//! Database connection pool using OnceLock pattern.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::OnceCell;

static POOL: OnceCell<PgPool> = OnceCell::const_new();

/// Get or initialize the database connection pool.
/// Uses SDB_ENDPOINT environment variable for the connection string.
pub async fn get_pool() -> Result<&'static PgPool, sqlx::Error> {
    POOL.get_or_try_init(|| async {
        dotenvy::dotenv().ok();

        let database_url = std::env::var("SDB_ENDPOINT")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("SDB_ENDPOINT or DATABASE_URL must be set");

        PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
    })
    .await
}
