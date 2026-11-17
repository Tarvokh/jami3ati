//! In-memory login throttling.
//!
//! A password endpoint is the one place where an attacker can spend guesses
//! cheaply, so the process keeps a small failure ledger. It is deliberately
//! **in-memory**: this is a hackathon-scale control plane with one process and
//! no Redis. A restart clears the ledger, which is acceptable because the
//! Argon2id cost still bounds the guess rate. A multi-instance deployment must
//! move this to shared storage; the interface is small enough to swap.
//!
//! The key is chosen by the caller (`email + ip` for login, `mailbox + ip` for
//! password changes). Keys are capped so an attacker cannot grow the map
//! without bound: the oldest window is evicted first.
//!
//! **Reserve before you verify.** [`LoginLimiter::try_attempt`] tests the
//! ceiling and increments under a single lock, so a concurrent burst cannot all
//! slip past a check-then-act race (the old `is_blocked` … Argon2 …
//! `record_failure` sequence let N parallel requests each buy a verify before
//! any of them counted). The caller reserves a slot, does the expensive
//! verification, and refunds with [`LoginLimiter::clear`] only on full success.
//!
//! Login also reserves against a coarse **per-IP** bucket
//! ([`LoginLimiter::try_attempt_capped`] with [`DEFAULT_IP_MAX_ATTEMPTS`]) whose
//! key does not contain the address. It bounds the total Argon2id work one
//! source can demand in a window no matter how many distinct — or non-existent —
//! addresses it tries, so throttling no longer depends on whether an account
//! exists. That bucket counts attempts and is never refunded by a success.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Eight failures per ten minutes, matching the identity module's contract.
const DEFAULT_WINDOW: Duration = Duration::from_secs(10 * 60);
const DEFAULT_MAX_FAILURES: u32 = 8;

/// Ceiling for the coarse per-IP login bucket. It counts *attempts* (not just
/// failures) and is never refunded by a success, so it caps the total Argon2id
/// work one source address can demand per window regardless of how many
/// distinct or non-existent addresses it sprays. Generous enough for a small
/// office behind one NAT; raise it for larger shared egress. Kept well above
/// [`DEFAULT_MAX_FAILURES`] so a single account's targeted brute force is still
/// caught by the tighter per-account bucket first.
pub const DEFAULT_IP_MAX_ATTEMPTS: u32 = 40;

/// Upper bound on tracked keys. Beyond this, the oldest windows are dropped.
const MAX_KEYS: usize = 10_000;

#[derive(Debug)]
struct Attempts {
    failures: u32,
    window_start: Instant,
}

#[derive(Debug)]
pub struct LoginLimiter {
    window: Duration,
    max_failures: u32,
    inner: Mutex<HashMap<String, Attempts>>,
}

impl LoginLimiter {
    pub fn new(window: Duration, max_failures: u32) -> Self {
        Self {
            window,
            max_failures: max_failures.max(1),
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn standard() -> Self {
        Self::new(DEFAULT_WINDOW, DEFAULT_MAX_FAILURES)
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Attempts>> {
        // A panic while holding the lock must not take the login page down for
        // the rest of the process: the ledger is advisory, not authoritative.
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// True when this key has reached the failure ceiling inside its window.
    pub fn is_blocked(&self, key: &str) -> bool {
        let mut map = self.lock();
        let Some(attempts) = map.get(key) else {
            return false;
        };
        if attempts.window_start.elapsed() >= self.window {
            map.remove(key);
            return false;
        }
        attempts.failures >= self.max_failures
    }

    /// Records one failure. A window that has expired starts over, so a user
    /// who mistyped eight times yesterday is not locked out today.
    pub fn record_failure(&self, key: &str) {
        let mut map = self.lock();
        self.evict_if_needed(&mut map);

        let now = Instant::now();
        match map.get_mut(key) {
            Some(attempts) if attempts.window_start.elapsed() < self.window => {
                attempts.failures = attempts.failures.saturating_add(1);
            }
            Some(attempts) => {
                attempts.failures = 1;
                attempts.window_start = now;
            }
            None => {
                map.insert(
                    key.to_string(),
                    Attempts {
                        failures: 1,
                        window_start: now,
                    },
                );
            }
        }
    }

    /// Atomically reserves one attempt against `key` using this limiter's own
    /// ceiling. Returns `false` **without incrementing** when the key is already
    /// at the ceiling inside its window; otherwise increments and returns
    /// `true`. Call this *before* the expensive password verification: the
    /// ceiling test and the increment happen under one lock, so concurrent
    /// requests cannot all pass a stale check. Refund the reserved attempt with
    /// [`LoginLimiter::clear`] only after full success; a wrong password leaves
    /// it counted.
    pub fn try_attempt(&self, key: &str) -> bool {
        self.try_attempt_capped(key, self.max_failures)
    }

    /// [`LoginLimiter::try_attempt`] with an explicit ceiling, so one ledger can
    /// hold both a tight per-account bucket and a looser per-IP bucket under
    /// disjoint key namespaces. `max` is floored at 1. An expired window starts
    /// over with this attempt counted as the first.
    pub fn try_attempt_capped(&self, key: &str, max: u32) -> bool {
        let max = max.max(1);
        let mut map = self.lock();
        self.evict_if_needed(&mut map);

        let now = Instant::now();
        match map.get_mut(key) {
            Some(attempts) if attempts.window_start.elapsed() < self.window => {
                if attempts.failures >= max {
                    return false;
                }
                attempts.failures = attempts.failures.saturating_add(1);
                true
            }
            Some(attempts) => {
                attempts.failures = 1;
                attempts.window_start = now;
                true
            }
            None => {
                map.insert(
                    key.to_string(),
                    Attempts {
                        failures: 1,
                        window_start: now,
                    },
                );
                true
            }
        }
    }

    /// Clears a key after a successful authentication, so one good password
    /// resets the count for that person and address.
    pub fn clear(&self, key: &str) {
        self.lock().remove(key);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.lock().len()
    }

    fn evict_if_needed(&self, map: &mut HashMap<String, Attempts>) {
        if map.len() < MAX_KEYS {
            return;
        }
        // Drop every expired window first; that is usually enough.
        map.retain(|_, attempts| attempts.window_start.elapsed() < self.window);
        if map.len() < MAX_KEYS {
            return;
        }
        // Still full: evict the oldest window. `MAX_KEYS` is small enough that
        // this sort runs only under deliberate pressure.
        let mut keys: Vec<(String, Instant)> = map
            .iter()
            .map(|(key, attempts)| (key.clone(), attempts.window_start))
            .collect();
        keys.sort_by_key(|(_, started)| *started);
        let excess = map.len() - MAX_KEYS + 1;
        for (key, _) in keys.into_iter().take(excess) {
            map.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_the_configured_number_of_failures() {
        let limiter = LoginLimiter::new(Duration::from_secs(600), 3);
        assert!(!limiter.is_blocked("aisha@uca.ma|127.0.0.1"));

        limiter.record_failure("aisha@uca.ma|127.0.0.1");
        limiter.record_failure("aisha@uca.ma|127.0.0.1");
        assert!(!limiter.is_blocked("aisha@uca.ma|127.0.0.1"));

        limiter.record_failure("aisha@uca.ma|127.0.0.1");
        assert!(limiter.is_blocked("aisha@uca.ma|127.0.0.1"));
    }

    #[test]
    fn a_success_clears_the_key() {
        let limiter = LoginLimiter::new(Duration::from_secs(600), 1);
        limiter.record_failure("key");
        assert!(limiter.is_blocked("key"));

        limiter.clear("key");
        assert!(!limiter.is_blocked("key"));
    }

    #[test]
    fn keys_are_independent() {
        let limiter = LoginLimiter::new(Duration::from_secs(600), 1);
        limiter.record_failure("aisha|10.0.0.1");
        assert!(limiter.is_blocked("aisha|10.0.0.1"));
        assert!(!limiter.is_blocked("aisha|10.0.0.2"));
        assert!(!limiter.is_blocked("yassine|10.0.0.1"));
    }

    #[test]
    fn an_expired_window_is_not_blocked() {
        let limiter = LoginLimiter::new(Duration::from_millis(1), 1);
        limiter.record_failure("key");
        std::thread::sleep(Duration::from_millis(5));
        assert!(!limiter.is_blocked("key"));
        // The expired entry is removed on the check, not left to grow.
        assert_eq!(limiter.len(), 0);
    }

    #[test]
    fn try_attempt_reserves_then_blocks_at_the_ceiling() {
        let limiter = LoginLimiter::new(Duration::from_secs(600), 3);
        assert!(limiter.try_attempt("key"));
        assert!(limiter.try_attempt("key"));
        assert!(limiter.try_attempt("key"));
        // Ceiling reached: the reserve is refused and does not increment.
        assert!(!limiter.try_attempt("key"));
        assert!(!limiter.try_attempt("key"));
        assert!(limiter.is_blocked("key"));
    }

    #[test]
    fn a_success_refunds_the_reserved_attempt() {
        let limiter = LoginLimiter::new(Duration::from_secs(600), 1);
        assert!(limiter.try_attempt("key"));
        assert!(!limiter.try_attempt("key"));
        // The good password refunds the slot, so the next attempt is allowed.
        limiter.clear("key");
        assert!(limiter.try_attempt("key"));
    }

    #[test]
    fn try_attempt_capped_honours_a_looser_ceiling() {
        let limiter = LoginLimiter::new(Duration::from_secs(600), 2);
        // The per-account ceiling is 2, but an explicit cap of 5 lets the same
        // ledger hold a looser per-IP bucket under a different key.
        for _ in 0..5 {
            assert!(limiter.try_attempt_capped("login-ip:10.0.0.1", 5));
        }
        assert!(!limiter.try_attempt_capped("login-ip:10.0.0.1", 5));
        // A disjoint account key is unaffected by the IP bucket.
        assert!(limiter.try_attempt("login:aisha@uca.ma|10.0.0.1"));
    }

    #[test]
    fn an_expired_window_starts_over_counting_this_attempt() {
        // The window must be far larger than the gap between two consecutive
        // in-process calls. A 1ms window flaked under load: the scheduler can
        // preempt the thread for >1ms between the reserve and the next assert,
        // expiring the window early so the "blocked" assert unexpectedly passed.
        // 200ms tolerates scheduler jitter; the 500ms sleep still proves expiry.
        let limiter = LoginLimiter::new(Duration::from_millis(200), 1);
        assert!(limiter.try_attempt("key"));
        assert!(!limiter.try_attempt("key"));
        std::thread::sleep(Duration::from_millis(500));
        // The window expired: the reserve succeeds and counts as the first of a
        // fresh window, so the ceiling applies again immediately.
        assert!(limiter.try_attempt("key"));
        assert!(!limiter.try_attempt("key"));
    }

    #[test]
    fn concurrent_reserves_cannot_exceed_the_ceiling() {
        // The check-then-act race this method exists to close: many threads
        // reserve at once, and exactly `ceiling` of them may proceed. With the
        // old is_blocked/record_failure split, every thread could pass the
        // check before any incremented.
        use std::sync::Arc;
        let ceiling = 8u32;
        let threads = 64usize;
        let limiter = Arc::new(LoginLimiter::new(Duration::from_secs(600), ceiling));

        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let limiter = Arc::clone(&limiter);
                std::thread::spawn(move || limiter.try_attempt("login-ip:10.0.0.9"))
            })
            .collect();

        let granted = handles
            .into_iter()
            .map(|h| h.join().expect("reserve thread must not panic"))
            .filter(|allowed| *allowed)
            .count();

        assert_eq!(granted as u32, ceiling);
        assert!(limiter.is_blocked("login-ip:10.0.0.9"));
        assert!(!limiter.try_attempt("login-ip:10.0.0.9"));
    }
}
