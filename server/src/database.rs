use sqlx::{Connection, PgConnection};

thread_local! {
    pub static DB: Connection = {
        // Open the database from the persisted "hotdog.db" file
        let conn = PgConnection::connect("postgres://postgres:postgres@localhost:5432/postgres")
            .expect("Failed to connect to the database.");

        // Create the "users" table if it doesn't already exist
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            );",
        ).unwrap();

        // Return the connection
        conn
    };
}