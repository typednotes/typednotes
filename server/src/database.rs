use tokio::runtime::Runtime;
use sqlx::{Connection, PgPool};

thread_local! {
    pub static PG_POOL: PgPool = {
        let runtime = Runtime::new().unwrap();
        let pool = runtime.block_on(async {
                // Connect to the database
            let pool = PgPool::connect("postgres://postgres:postgres@localhost:5432/postgres").await
                .expect("Failed to connect to the database.");

            // // Create the "users" table if it doesn't already exist
            // sqlx::query(
            //     "CREATE TABLE IF NOT EXISTS users (
            //         id INTEGER PRIMARY KEY,
            //         name TEXT NOT NULL
            //     );",
            // ).execute(&pool).await.unwrap();

            // // Example query: Select all rows from the `users` table
            // let rows = sqlx::query("SELECT id, name, email FROM users")
            //     .fetch_all(&pool)
            //     .await.unwrap();

            // // Iterate over the rows and print the results
            // for row in rows {
            //     let id: i32 = row.get("id");
            //     let name: String = row.get("name");
            //     let email: String = row.get("email");

            //     println!("ID: {}, Name: {}, Email: {}", id, name, email);
            // }
            pool
        });
        // Return the connection
        pool
    };
}

pub async fn test(input: &str) {
    println!("Hello world {input}");
    // sqlx::query("INSERT INTO users (name) VALUES ($1)").bind(input).execute(&PG_POOL).await.unwrap();
}