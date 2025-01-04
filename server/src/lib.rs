//! This crate contains all shared fullstack server functions.
#[cfg(feature = "server")]
mod application;
#[cfg(feature = "server")]
mod database;
#[cfg(feature = "server")]
mod user;
#[cfg(feature = "server")]
mod auth;
mod utils;

#[cfg(feature = "server")]
pub use application::launch;


