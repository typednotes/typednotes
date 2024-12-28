//! This crate contains all shared fullstack server functions.
use dioxus::prelude::*;
#[cfg(feature = "server")]
mod database;

/// Echo the user input on the server.
#[server(Echo)]
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    // database::DB.with(|db| async {
    //     // Example query: Select all rows from the `users` table
    //     let rows = sqlx::query("SELECT id, name, email FROM users")
    //         .fetch_all(db)
    //         .await.unwrap();

    //     // // Iterate over the rows and print the results
    //     // for row in rows {
    //     //     let id: i32 = row.get("id");
    //     //     let name: String = row.get("name");
    //     //     let email: String = row.get("email");

    //     //     println!("ID: {}, Name: {}, Email: {}", id, name, email);
    //     // }
    // }).await;
    sqlx::query("SELECT id, name, email FROM users")
        .fetch_all(&database::get_connection_pool().await?)
        .await.unwrap();
    Ok(format!("Hello world {input}"))
}
