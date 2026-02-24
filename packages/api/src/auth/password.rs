//! # Password hashing and verification — Argon2id
//!
//! Provides the two functions used by the local (email + password) authentication path:
//!
//! - [`hash_password`] — generates a random salt via [`OsRng`], hashes the plaintext
//!   password with the default Argon2id parameters, and returns the result as a
//!   PHC-format string (e.g. `$argon2id$v=19$m=19456,t=2,p=1$...`). This string is
//!   stored in the `password_hash` column of the `users` table.
//!
//! - [`verify_password`] — parses a PHC-format hash and checks whether the provided
//!   plaintext matches. Returns `Ok(true)` on success, `Ok(false)` on mismatch, or
//!   `Err` if the stored hash is malformed.
//!
//! Both functions use the `argon2` crate with its default (memory-hard) configuration,
//! which provides strong resistance against GPU and ASIC brute-force attacks.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

/// Hash a password using Argon2id. Returns a PHC-format string.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("Failed to hash password: {}", e))?;
    Ok(hash.to_string())
}

/// Verify a password against a PHC-format hash string.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| format!("Invalid password hash: {}", e))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
