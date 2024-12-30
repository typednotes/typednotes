//! This crate contains all shared fullstack server functions.
use dioxus::prelude::*;
#[cfg(feature = "server")]
mod application;
#[cfg(feature = "server")]
mod database;
#[cfg(feature = "server")]
mod user;
#[cfg(feature = "server")]
mod auth;

#[cfg(feature = "server")]
pub use application::launch;

/// Echo the user input on the server.
#[server(Echo)]
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    let connection_pool = database::connection_pool()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    // sqlx::query("SELECT id, name, email FROM users")
    //     .fetch_all()
    //     .await.unwrap();
    // let mut conn = <impl sqlx::Executor>;
    let account = sqlx::query!("select (1) as id, 'Herp Derpinson' as name")
        .fetch_one(connection_pool)
        .await?;
    Ok(format!("Hello world {input}"))
}

