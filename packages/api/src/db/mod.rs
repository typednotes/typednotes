//! # Database module — PostgreSQL connection pool management
//!
//! This module provides the shared PostgreSQL connection pool used by every server
//! function in the `api` crate. It is entirely gated behind `#[cfg(feature = "server")]`
//! so that client (WASM) builds never pull in SQLx or Tokio networking code.
//!
//! ## Design
//!
//! The pool is a **lazy, process-wide singleton** backed by a [`tokio::sync::OnceCell`].
//! The first call to [`get_pool`] reads `DATABASE_URL` from the environment (via `dotenvy`),
//! opens a connection pool with up to 5 connections, and caches the result for all
//! subsequent callers.
//!
//! ## Re-exports
//!
//! - [`get_pool`] — returns `&'static PgPool`, initialising it on first use.

#[cfg(feature = "server")]
mod pool;

#[cfg(feature = "server")]
pub use pool::get_pool;
