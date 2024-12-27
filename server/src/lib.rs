//! This crate contains all shared fullstack server functions.
use dioxus::prelude::*;
#[cfg(feature = "server")]
mod database;

/// Echo the user input on the server.
#[server(Echo)]
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    // #[cfg(feature = "server")]
    // database::DB.execute("INSERT INTO users (name) VALUES ($1)", &input).await?;
    Ok(format!("Hello world {input}"))
}
