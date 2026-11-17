//! Bounds on the Argon2id work factor accepted from a *stored* hash (M-01).
//!
//! Why this exists: `password-hash`'s blanket `PasswordVerifier` impl does not
//! verify with the verifier's own defaults. It re-derives using the parameters
//! embedded in the PHC string (`Params::try_from(hash)`), then compares. So a
//! stored hash that declares `m=4294967295,t=4294967295,p=16777215` would make
//! `verify_password` try to allocate ~4 TiB and run billions of iterations — a
//! denial of service triggered by anyone who can plant or corrupt a hash row
//! (a registration field, a profile update, a compromised write).
//!
//! The defence is to inspect the parsed parameters and refuse anything above a
//! ceiling a login can afford, *before* calling the verifier. The production
//! defaults (m=19456 KiB, t=2, p=1) sit well inside the caps, and the headroom
//! lets the cost be raised later without rejecting legitimate hashes.

use argon2::password_hash::PasswordHash;
use argon2::Params;

/// Memory ceiling: 64 MiB. The default is 19 MiB; this leaves room to harden.
pub(crate) const MAX_M_COST: u32 = 65_536;
/// Iteration ceiling. Default 2; cap 4.
pub(crate) const MAX_T_COST: u32 = 4;
/// Parallelism ceiling. Default 1; cap 4.
pub(crate) const MAX_P_COST: u32 = 4;

/// True when `parsed` carries Argon2 parameters within the accepted bounds.
///
/// Returns `false` when the parameters cannot be extracted (malformed hash) or
/// exceed any cap, so the caller rejects the hash without an expensive
/// derivation. This is a pure check: it never hashes, allocates work memory,
/// or touches the password.
pub(crate) fn params_are_bounded(parsed: &PasswordHash<'_>) -> bool {
    let Ok(params) = Params::try_from(parsed) else {
        return false;
    };
    params.m_cost() <= MAX_M_COST && params.t_cost() <= MAX_T_COST && params.p_cost() <= MAX_P_COST
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::password::hash_password;

    /// Swaps the parameter segment (index 3 of a `$`-split PHC string) while
    /// keeping the algorithm, version, salt and output structurally valid.
    fn reparam(params: &str) -> String {
        let real = hash_password("candidate").expect("hash");
        let mut parts: Vec<String> = real.split('$').map(str::to_string).collect();
        assert!(parts.len() >= 6, "unexpected PHC shape: {real}");
        parts[3] = params.to_string();
        parts.join("$")
    }

    fn bounded(params: &str) -> bool {
        let phc = reparam(params);
        let parsed = PasswordHash::new(&phc).expect("parse");
        params_are_bounded(&parsed)
    }

    #[test]
    fn defaults_are_within_bounds() {
        assert!(bounded("m=19456,t=2,p=1"));
    }

    #[test]
    fn values_at_the_caps_are_accepted() {
        assert!(bounded("m=65536,t=4,p=4"));
    }

    #[test]
    fn any_value_one_over_a_cap_is_rejected() {
        assert!(!bounded("m=65537,t=2,p=1"));
        assert!(!bounded("m=19456,t=5,p=1"));
        assert!(!bounded("m=19456,t=2,p=5"));
    }

    #[test]
    fn absurd_work_factors_are_rejected() {
        assert!(!bounded("m=4294967295,t=4294967295,p=16777215"));
    }
}
