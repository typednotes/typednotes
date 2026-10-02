//! Server-only half of the `api` crate. Compiled only with the `server`
//! feature: the wasm client never links a database driver, an HTTP client,
//! or any of the crypto below.

pub mod account;
pub mod ai;
pub mod channels;
pub mod compute;
pub mod config;
pub mod connector;
pub mod connections;
pub mod db;
pub mod errors;
pub mod graphs;
pub mod liaison;
pub mod lode;
pub mod local;
pub mod lun;
pub mod members;
pub mod oauth;
pub mod projects;
pub mod routes;
pub mod rpc;
pub mod scheduler;
pub mod session;
pub mod vault;
pub mod warrant;
pub mod workspace;
pub mod shares;
