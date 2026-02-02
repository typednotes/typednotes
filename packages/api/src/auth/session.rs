//! Session data types.

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
