//! One-time codes for phone login.
//!
//! Threat model: an attacker who can read the database must not be able to
//! recover live codes, and must not be able to brute-force a 6-digit code.
//! Therefore the code is stored as an Argon2id hash (not plaintext, not a
//! fast SHA-256), and every attempt is counted.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::Rng;

use crate::AuthError;

pub const CODE_DIGITS: u32 = 6;
pub const DEFAULT_TTL: Duration = Duration::from_secs(5 * 60);
pub const DEFAULT_MAX_ATTEMPTS: u8 = 5;

/// Server-side record. Persist this, never the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpChallenge {
    code_hash: String,
    expires_at_unix: u64,
    attempts_left: u8,
}

/// What the SMS gateway receives.
#[derive(Clone)]
pub struct IssuedOtp {
    pub code: String,
    pub challenge: OtpChallenge,
}

impl std::fmt::Debug for IssuedOtp {
    /// Never prints the plaintext code. A derived `Debug` would render
    /// `code: "123456"`, so any `tracing::debug!(?issued)` or panic message
    /// would leak a live one-time code into the logs. The challenge is safe to
    /// show (it holds only the Argon2id hash, expiry, and attempt counter).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedOtp")
            .field("code", &"<redacted>")
            .field("challenge", &self.challenge)
            .finish()
    }
}

impl OtpChallenge {
    pub fn expires_at_unix(&self) -> u64 {
        self.expires_at_unix
    }

    pub fn attempts_left(&self) -> u8 {
        self.attempts_left
    }

    pub fn is_expired_at(&self, now_unix: u64) -> bool {
        now_unix >= self.expires_at_unix
    }

    /// Fail-closed expiry decision given a clock reading. An unreadable clock
    /// (system time before the epoch, or otherwise unavailable) must never
    /// *extend* a code's life, so it is treated as "expired" and the code is
    /// refused. Splitting the decision from the clock read keeps this branch
    /// deterministically testable without mocking the system clock.
    fn expired_given(&self, now: Result<u64, AuthError>) -> bool {
        match now {
            Ok(now) => self.is_expired_at(now),
            Err(_) => true,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expired_given(now_unix())
    }

    /// Verifies and burns one attempt. Fails closed on expiry or exhaustion.
    ///
    /// A successful verification burns the whole challenge (H-05): the code is
    /// single-use, so `attempts_left` is zeroed and the stored hash is cleared.
    /// Without this the same code would verify again on every remaining
    /// attempt within its TTL — a replayable "one-time" code. Clearing the hash
    /// also removes the secret material, so a database read after a successful
    /// login yields nothing to brute-force.
    pub fn verify(&mut self, candidate: &str) -> bool {
        if self.attempts_left == 0 || self.is_expired() {
            return false;
        }
        self.attempts_left -= 1;

        let Ok(parsed) = PasswordHash::new(&self.code_hash) else {
            return false;
        };
        // M-01: refuse an absurd stored work factor before the verifier
        // re-derives with the hash's own parameters.
        if !crate::work_factor::params_are_bounded(&parsed) {
            return false;
        }
        let ok = Argon2::default()
            .verify_password(candidate.as_bytes(), &parsed)
            .is_ok();
        if ok {
            self.attempts_left = 0;
            self.code_hash.clear();
        }
        ok
    }
}

/// Generates a code and the matching server-side challenge.
pub fn issue(ttl: Duration, max_attempts: u8) -> Result<IssuedOtp, AuthError> {
    // Read the clock once, up front, and fail closed if it is unreadable:
    // issuing a code against a bogus expiry (the old `unwrap_or(0)`) would
    // create a challenge that never expires.
    let now = now_unix()?;

    let code = format!(
        "{:0width$}",
        rand::thread_rng().gen_range(0..10u32.pow(CODE_DIGITS)),
        width = CODE_DIGITS as usize
    );

    let salt = SaltString::generate(&mut OsRng);
    let code_hash = Argon2::default()
        .hash_password(code.as_bytes(), &salt)
        .map_err(|_| AuthError::Hash)?
        .to_string();

    Ok(IssuedOtp {
        code,
        challenge: OtpChallenge {
            code_hash,
            expires_at_unix: now.saturating_add(ttl.as_secs()),
            attempts_left: max_attempts,
        },
    })
}

/// Seconds since the Unix epoch, or `AuthError::Clock` if the system time is
/// unreadable (e.g. set before 1970). Callers must fail closed on `Err`: a
/// broken clock is never a reason to accept or extend a one-time code.
fn now_unix() -> Result<u64, AuthError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| AuthError::Clock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_six_digits_and_verifies() {
        let issued = issue(DEFAULT_TTL, DEFAULT_MAX_ATTEMPTS).expect("issue");
        assert_eq!(issued.code.len(), 6);
        assert!(issued.code.chars().all(|c| c.is_ascii_digit()));

        let mut challenge = issued.challenge;
        assert!(challenge.verify(&issued.code));
    }

    #[test]
    fn wrong_code_fails_and_burns_attempts() {
        let issued = issue(DEFAULT_TTL, 2).expect("issue");
        let mut challenge = issued.challenge;
        assert!(!challenge.verify("000000"));
        assert_eq!(challenge.attempts_left(), 1);
        assert!(!challenge.verify("111111"));
        assert_eq!(challenge.attempts_left(), 0);
        // Even the right code is refused once attempts are exhausted.
        assert!(!challenge.verify(&issued.code));
    }

    #[test]
    fn expired_code_is_refused() {
        let issued = issue(Duration::ZERO, 3).expect("issue");
        let mut challenge = issued.challenge;
        assert!(challenge.is_expired());
        assert!(!challenge.verify(&issued.code));
    }

    #[test]
    fn codes_are_not_stored_in_plaintext() {
        let issued = issue(DEFAULT_TTL, 3).expect("issue");
        let debug = format!("{:?}", issued.challenge);
        assert!(!debug.contains(&issued.code));
    }

    #[test]
    fn successful_verification_is_single_use() {
        let issued = issue(DEFAULT_TTL, DEFAULT_MAX_ATTEMPTS).expect("issue");
        let mut challenge = issued.challenge;

        assert!(challenge.verify(&issued.code), "the first use must succeed");
        assert_eq!(
            challenge.attempts_left(),
            0,
            "success must burn the challenge, not leave attempts for a replay"
        );
        assert!(
            !challenge.verify(&issued.code),
            "the same code must not verify a second time (H-05)"
        );
        assert!(!challenge.verify(&issued.code), "nor a third");
    }

    #[test]
    fn success_clears_the_stored_hash_material() {
        let issued = issue(DEFAULT_TTL, 3).expect("issue");
        let mut challenge = issued.challenge;
        assert!(challenge.verify(&issued.code));

        // After a successful login the server-side record holds no hash to
        // brute-force and no plaintext code.
        let debug = format!("{:?}", challenge);
        assert!(
            !debug.contains("$argon2id$"),
            "hash material must be cleared"
        );
        assert!(!debug.contains(&issued.code));
    }

    #[test]
    fn wrong_code_burns_only_one_attempt_not_the_whole_challenge() {
        let issued = issue(DEFAULT_TTL, 3).expect("issue");
        let mut challenge = issued.challenge;
        // A wrong guess must not clear the hash: the right code still works
        // afterwards, until attempts run out.
        assert!(!challenge.verify("000000"));
        assert_eq!(challenge.attempts_left(), 2);
        assert!(
            challenge.verify(&issued.code),
            "right code still works after a miss"
        );
        assert_eq!(challenge.attempts_left(), 0, "then success burns it");
    }

    #[test]
    fn otp_verify_rejects_absurd_stored_work_factor() {
        let issued = issue(DEFAULT_TTL, 3).expect("issue");
        // The test module is a child of `otp`, so it can plant a hostile hash
        // directly to prove the M-01 guard is wired into this verify path too.
        let mut parts: Vec<String> = issued
            .challenge
            .code_hash
            .split('$')
            .map(str::to_string)
            .collect();
        assert!(parts.len() >= 6, "unexpected PHC shape");
        parts[3] = "m=4294967295,t=4294967295,p=16777215".to_string();
        let mut challenge = OtpChallenge {
            code_hash: parts.join("$"),
            expires_at_unix: now_unix().expect("clock") + 300,
            attempts_left: 3,
        };
        assert!(
            !challenge.verify("000000"),
            "an absurd work factor must be refused without derivation"
        );
    }

    #[test]
    fn issued_otp_debug_redacts_the_plaintext_code() {
        let issued = issue(DEFAULT_TTL, 3).expect("issue");
        // The whole point of the manual Debug: a live code must never reach a
        // log line or panic message via `{:?}` on the issued payload.
        let debug = format!("{issued:?}");
        assert!(
            !debug.contains(&issued.code),
            "IssuedOtp Debug leaked the plaintext code: {debug}"
        );
        assert!(debug.contains("<redacted>"), "code field must be redacted");
        // The challenge (hash/expiry/attempts) is still inspectable.
        assert!(debug.contains("challenge"), "challenge must remain visible");
    }

    #[test]
    fn a_broken_clock_fails_closed_not_open() {
        let issued = issue(DEFAULT_TTL, 3).expect("issue");
        let challenge = issued.challenge;

        // A readable clock before expiry: the code is still live.
        assert!(!challenge.expired_given(Ok(challenge.expires_at_unix() - 1)));
        // A readable clock at/after expiry: expired.
        assert!(challenge.expired_given(Ok(challenge.expires_at_unix())));
        // An unreadable clock must fail closed (treated as expired), never
        // fail open. The old `unwrap_or(0)` made every code immortal here.
        assert!(
            challenge.expired_given(Err(AuthError::Clock)),
            "a clock error must refuse the code, not extend its life"
        );
    }
}
