//! # Session data types and constants
//!
//! Defines the lightweight structures used to track authenticated users across requests
//! via `tower-sessions`.
//!
//! - [`SESSION_USER_ID_KEY`] — the string key (`"user_id"`) under which the authenticated
//!   user's UUID is stored in the session. Every server function that needs authentication
//!   reads this key from the [`tower_sessions::Session`] to identify the caller.
//!
//! - [`SessionData`] — a typed wrapper around the session payload. Currently holds only an
//!   optional `user_id`; defaults to `None` (unauthenticated). It is `Serialize + Deserialize`
//!   so it can be persisted by the PostgreSQL-backed session store
//!   (`tower-sessions-sqlx-store`).

use serde::{Deserialize, Serialize};

/// Key for storing user ID in session.
pub const SESSION_USER_ID_KEY: &str = "user_id";

/// Session data stored in the session store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub user_id: Option<String>,
}

impl Default for SessionData {
    fn default() -> Self {
        Self { user_id: None }
    }
}
