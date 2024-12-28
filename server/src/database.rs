use std::env;
use tokio::{runtime::Runtime, sync::OnceCell};
use sqlx::{Connection, PgPool};
use anyhow::{Context, Result};

pub static CONNECTION_POOL: OnceCell<PgPool> = OnceCell::const_new();

/// Get a connection pool to the database
async fn init_connection_pool() -> Result<()> {
    let user = env::var("POSTGRES_USER")?;
    let password = env::var("POSTGRES_PASSWORD")?;
    let host = env::var("POSTGRES_HOST")?;
    let port = env::var("POSTGRES_PORT")?;
    let database = env::var("POSTGRES_DB")?;
    let url = format!("postgres://{user}:{password}@{host}:{port}/{database}");
    let pool = PgPool::connect(&url).await?;
    CONNECTION_POOL.set(pool).context("Failed to set global pool")?;
    Ok(())
}

async fn get_connection_pool() -> Result<PgPool> {
    CONNECTION_POOL.get_or_try_init(init_connection_pool).await?;
    Ok(CONNECTION_POOL.get().unwrap().clone())
}

/// Get a connection pool to the database
async fn init_db() -> Result<PgPool> {
    let connection_pool = get_connection_pool().await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        );",
    ).execute(&connection_pool).await.context("Failed to create table")?;
    Ok(pool)
}
