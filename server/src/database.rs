use anyhow::Context as _;
use sqlx::{Connection, Database, PgPool, Pool};
use std::env;
use tokio::{runtime::Runtime, sync::OnceCell};
use crate::settings::{self, Settings};

pub static CONNECTION_POOL: OnceCell<PgPool> = OnceCell::const_new();

/// Initialize the database with tables if they don't exist
async fn init_database(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        );",
    )
    .execute(pool)
    .await
    .context("Failed to create table")?;
    Ok(())
}

/// Initialize a connection pool to the database
async fn init_connection_pool() -> anyhow::Result<PgPool> {
    let settings = Settings::new().context("Read config")?;
    let url = settings.database.url();
    let pool = PgPool::connect(&url).await?;
    // Create the tables if they don't exist
    init_database(&pool).await?;
    Ok(pool)
}

/// Get the connection pool
pub async fn connection_pool() -> anyhow::Result<&'static PgPool> {
    Ok(CONNECTION_POOL
        .get_or_try_init(init_connection_pool)
        .await?)
}
