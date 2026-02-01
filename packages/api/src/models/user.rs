//! User model for authenticated users.

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
