//! Server-only half of the `api` crate. Compiled only with the `server`
//! feature: the wasm client never links a database driver, an HTTP client,
//! or any of the crypto below.

pub mod channels;
pub mod config;
pub mod connections;
pub mod db;
pub mod errors;
pub mod liaison;
pub mod oauth;
pub mod projects;
pub mod routes;
pub mod session;
pub mod vault;
pub mod warrant;
