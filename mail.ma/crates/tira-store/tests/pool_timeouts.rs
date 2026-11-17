//! Pool checkout bounds (Low: no pool timeouts).
//!
//! `db::build_pool` used to leave every deadpool timeout at `None`, so a
//! checkout could block forever when the database stalled or every slot was
//! held by a slow query. These tests prove the bounds are wired: a default
//! config sets all four, and an exhausted one-slot pool fails a second
//! checkout fast with a *wait* timeout instead of hanging.
//!
//! The behavioural test needs a real PostgreSQL; `TIRA_REQUIRE_DATABASE=1`
//! turns a missing database into a failure instead of a silent skip.

use std::time::{Duration, Instant};

use deadpool_postgres::{PoolError, TimeoutType};
use tira_store::db::{build_pool_with_timeouts, PoolTimeouts};

fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty());

    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!(
            "TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set; \
             refusing to report a green run with zero pool-timeout coverage"
        );
    }

    url
}

/// No database needed: the default config must bound every blocking path, and
/// the connect cap must not exceed the create cap (the more specific bound
/// should fire first).
#[test]
fn default_timeouts_bound_every_checkout_path() {
    let t = PoolTimeouts::default();
    assert!(t.connect > Duration::ZERO, "connect must be bounded");
    assert!(t.wait > Duration::ZERO, "wait must be bounded");
    assert!(t.create > Duration::ZERO, "create must be bounded");
    assert!(t.recycle > Duration::ZERO, "recycle must be bounded");
    assert!(
        t.connect <= t.create,
        "the connect cap should fire before the outer create cap"
    );
}

#[tokio::test]
async fn wait_timeout_bounds_a_checkout_when_the_pool_is_exhausted() {
    let Some(url) = database_url() else {
        return;
    };

    // A one-slot pool with a millisecond-scale wait bound: holding the only
    // connection must make a second checkout fail fast, not block forever.
    let timeouts = PoolTimeouts {
        wait: Duration::from_millis(80),
        ..PoolTimeouts::default()
    };
    let pool = build_pool_with_timeouts(&url, 1, timeouts).expect("build pool");

    let held = pool
        .get()
        .await
        .expect("first checkout takes the only slot");

    let started = Instant::now();
    let err = pool
        .get()
        .await
        .expect_err("an exhausted pool must time out, not block forever");
    let elapsed = started.elapsed();
    assert!(
        matches!(err, PoolError::Timeout(TimeoutType::Wait)),
        "expected a wait timeout, got {err:?}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "wait_timeout did not bound the checkout: took {elapsed:?}"
    );

    // Returning the slot makes the pool usable again.
    drop(held);
    let _again = pool
        .get()
        .await
        .expect("checkout succeeds once the slot is free");
}
