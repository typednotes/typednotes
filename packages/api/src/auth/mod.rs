//! # Authentication module — OAuth, local passwords, and session management
//!
//! This module implements every authentication strategy supported by TypedNotes.
//! All submodules are gated behind `#[cfg(feature = "server")]` because authentication
//! logic runs exclusively on the Axum server; client builds only interact with it
//! through the server functions defined in [`crate`].
//!
//! ## Submodules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`config`] | Reads OAuth client credentials from environment variables and builds [`OAuthConfig`] structs |
//! | [`github`] | GitHub OAuth 2.0 flow — authorization URL generation, code exchange, user upsert |
//! | [`google`] | Google OAuth 2.0 flow — same pattern as GitHub with OpenID Connect scopes |
//! | [`password`] | Argon2id password hashing and verification for local (email+password) accounts |
//! | [`session`] | Session data types and the [`SESSION_USER_ID_KEY`] constant used across the crate |
//!
//! ## OAuth flow overview
//!
//! 1. The frontend calls `get_login_url(provider)` which delegates to
//!    [`GitHubOAuth::generate_auth_url`] or [`GoogleOAuth::generate_auth_url`].
//! 2. The handler creates a PKCE challenge, persists the CSRF state + verifier in the
//!    `oauth_states` table (with a 10-minute TTL), and returns the authorization URL.
//! 3. After the user consents, the provider redirects to `/auth/{provider}/callback`
//!    (handled in the `web` crate) which calls `exchange_code` to trade the authorization
//!    code for an access token, fetch the user profile, and upsert the `users` row.
//! 4. The callback stores the user ID in the `tower-sessions` session so subsequent
//!    server functions can authenticate the caller.

#[cfg(feature = "server")]
mod config;
#[cfg(feature = "server")]
mod github;
#[cfg(feature = "server")]
mod google;
#[cfg(feature = "server")]
mod password;
#[cfg(feature = "server")]
mod session;

#[cfg(feature = "server")]
pub use config::OAuthConfig;
#[cfg(feature = "server")]
pub use password::{hash_password, verify_password};
#[cfg(feature = "server")]
pub use github::GitHubOAuth;
#[cfg(feature = "server")]
pub use google::GoogleOAuth;
#[cfg(feature = "server")]
pub use session::{SessionData, SESSION_USER_ID_KEY};
