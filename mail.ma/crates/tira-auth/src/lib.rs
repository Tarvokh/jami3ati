//! Tira Mail authentication primitives.
//!
//! - [`password`]: Argon2id password hashing.
//! - [`otp`]: short-lived, attempt-limited one-time codes (phone login).
//!
//! Deliberately no HTTP, no database, no SMS provider here: this crate is
//! pure policy so it can be tested without infrastructure.

pub mod otp;
pub mod password;
mod work_factor;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("hashing failed")]
    Hash,
    #[error("invalid stored hash")]
    InvalidHash,
    #[error("system clock is unreadable")]
    Clock,
}

pub const NAME: &str = "tira-auth";
