//! This crate contains all shared fullstack server functions.
#[cfg(feature = "server")]
mod settings;
#[cfg(feature = "server")]
mod application;
#[cfg(feature = "server")]
mod database;
#[cfg(feature = "server")]
mod user;
#[cfg(feature = "server")]
mod auth;

use dioxus::prelude::*;
#[cfg(feature = "server")]
pub use application::launch;

/// Echo the user input on the server.
#[server(Echo)]
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    Ok(format!("Hello world {input}"))
}

/// Echo the user input on the server.
#[server(Test)]
pub async fn test(input: String) -> Result<String, ServerFnError> {    
    Ok(format!("Test {input}"))
}