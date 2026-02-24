//! # User model for authenticated users
//!
//! Defines the two representations of a TypedNotes user:
//!
//! ## [`User`] (server only)
//!
//! The complete database row from the `users` table. It derives [`sqlx::FromRow`] so it
//! can be loaded directly from queries and contains every column:
//!
//! - `id` — primary key (`UUID v4`).
//! - `email`, `name`, `avatar_url` — profile fields populated during OAuth or registration.
//! - `provider` / `provider_id` — identify the auth provider (`"github"`, `"google"`, or
//!   `"local"` for email+password accounts where `provider_id` equals the email).
//! - `password_hash` — Argon2 hash, present only for `"local"` accounts.
//! - `created_at` / `updated_at` — audit timestamps.
//!
//! The [`User::to_info`] method projects this into a [`UserInfo`].
//!
//! ## [`UserInfo`]
//!
//! A client-safe subset that is `Serialize + Deserialize + PartialEq` and can cross the
//! server/client boundary via Dioxus server functions. It omits the password hash and
//! timestamps and converts the `Uuid` to a `String` so it works in WASM.
//! The helper [`UserInfo::display_name`] returns the user's name or falls back to their
//! email address.

use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use chrono::{DateTime, Utc};
#[cfg(feature = "server")]
use sqlx::FromRow;
#[cfg(feature = "server")]
use uuid::Uuid;

/// Full user record from the database.
#[cfg(feature = "server")]
#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub provider: String,
    pub provider_id: String,
    pub password_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "server")]
impl User {
    /// Convert to UserInfo for client consumption.
    pub fn to_info(&self) -> UserInfo {
        UserInfo {
            id: self.id.to_string(),
            email: self.email.clone(),
            name: self.name.clone(),
            avatar_url: self.avatar_url.clone(),
            provider: self.provider.clone(),
        }
    }
}

/// User information safe to send to the client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub provider: String,
}

impl UserInfo {
    /// Get display name, falling back to email if name is not set.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.email)
    }
}
