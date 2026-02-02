//! Authentication module for OAuth providers.

#[cfg(feature = "server")]
mod config;
#[cfg(feature = "server")]
mod github;
#[cfg(feature = "server")]
mod google;
#[cfg(feature = "server")]
mod session;

#[cfg(feature = "server")]
pub use config::OAuthConfig;
#[cfg(feature = "server")]
pub use github::GitHubOAuth;
#[cfg(feature = "server")]
pub use google::GoogleOAuth;
#[cfg(feature = "server")]
pub use session::{SessionData, SESSION_USER_ID_KEY};
