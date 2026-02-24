//! # Data models for the TypedNotes application
//!
//! This module contains the domain models that represent persisted entities.
//! Each model typically comes in two flavours:
//!
//! - A **full database record** (e.g. [`User`]), gated behind `#[cfg(feature = "server")]`,
//!   that derives [`sqlx::FromRow`] and carries all columns including sensitive or
//!   server-internal fields (`password_hash`, timestamps, UUIDs).
//!
//! - A **client-safe projection** (e.g. [`UserInfo`]), available on every target, that
//!   derives `Serialize`/`Deserialize` and contains only the fields the frontend needs.
//!   These are the types returned by Dioxus server functions and consumed by UI components.
//!
//! ## Re-exports
//!
//! - [`User`] (server only) — full user row from the `users` table.
//! - [`UserInfo`] — lightweight, serializable user data for the client.

mod user;

#[cfg(feature = "server")]
pub use user::User;
pub use user::UserInfo;
