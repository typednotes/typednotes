//! Data models for the application.

mod user;

#[cfg(feature = "server")]
pub use user::User;
pub use user::UserInfo;
