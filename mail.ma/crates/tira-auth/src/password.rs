//! Password hashing with Argon2id (OWASP-recommended parameters via
//! `argon2::Argon2::default()`: m=19456 KiB, t=2, p=1).

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

use crate::AuthError;

/// Returns a PHC string (`$argon2id$v=19$...`) safe to store as-is.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::Hash)
}

/// Constant-time verification. Returns `false` for malformed stored hashes
/// instead of erroring, so callers cannot distinguish the two cases.
///
/// A stored hash whose Argon2 work factor exceeds the accepted ceiling is also
/// rejected (`false`) *before* any derivation: the verifier re-hashes with the
/// parameters embedded in the hash, so an absurd `m`/`t`/`p` would otherwise
/// be a denial-of-service lever (M-01). See [`crate::work_factor`].
pub fn verify_password(password: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    if !crate::work_factor::params_are_bounded(&parsed) {
        return false;
    }
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies() {
        let hash = hash_password("Str0ng-Passphrase!").expect("hash");
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("Str0ng-Passphrase!", &hash));
    }

    #[test]
    fn rejects_wrong_password() {
        let hash = hash_password("right").expect("hash");
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn hashes_are_salted() {
        let a = hash_password("same").expect("hash");
        let b = hash_password("same").expect("hash");
        assert_ne!(a, b);
        assert!(verify_password("same", &a));
        assert!(verify_password("same", &b));
    }

    #[test]
    fn malformed_hash_does_not_panic() {
        assert!(!verify_password("x", "not-a-phc-string"));
        assert!(!verify_password("x", ""));
    }

    /// Swaps the parameter segment (index 3 of a `$`-split PHC string), keeping
    /// the salt and output structurally valid so only the work factor changes.
    fn reparam(real: &str, params: &str) -> String {
        let mut parts: Vec<String> = real.split('$').map(str::to_string).collect();
        assert!(parts.len() >= 6, "unexpected PHC shape: {real}");
        parts[3] = params.to_string();
        parts.join("$")
    }

    #[test]
    fn absurd_stored_work_factor_is_rejected_before_derivation() {
        let real = hash_password("candidate").expect("hash");
        // Without the M-01 guard the verifier would try to allocate ~4 TiB and
        // run billions of iterations. The guard must reject it instantly.
        let hostile = reparam(&real, "m=4294967295,t=4294967295,p=16777215");
        assert_ne!(hostile, real, "the parameter swap must have taken effect");
        assert!(
            !verify_password("candidate", &hostile),
            "an absurd work factor must be refused, not computed"
        );
    }

    #[test]
    fn stronger_but_bounded_hash_still_verifies() {
        use argon2::password_hash::PasswordHasher;
        use argon2::{Argon2, Params};

        // 64 MiB / t=2 / p=1 sits exactly at the memory cap: a legitimate
        // hardening must not be rejected by the guard.
        let params = Params::new(65_536, 2, 1, None).expect("params");
        let hasher = Argon2::from(params);
        let salt = SaltString::generate(&mut OsRng);
        let strong = hasher
            .hash_password(b"candidate", &salt)
            .expect("hash")
            .to_string();
        assert!(
            strong.contains("m=65536"),
            "hash must carry the strong cost"
        );
        assert!(
            verify_password("candidate", &strong),
            "a bounded 64 MiB hash must still verify"
        );
        assert!(!verify_password("wrong", &strong));
    }
}
