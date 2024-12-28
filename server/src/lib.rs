//! This crate contains all shared fullstack server functions.
use dioxus::prelude::*;
#[cfg(feature = "server")]
mod database;

/// Echo the user input on the server.
#[server(Echo)]
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    let connection_pool = database::get_connection_pool().await.map_err(|e| ServerFnError::new(e.to_string()))?;
    // sqlx::query("SELECT id, name, email FROM users")
    //     .fetch_all()
    //     .await.unwrap();
    Ok(format!("Hello world {input}"))
}
