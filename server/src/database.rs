use anyhow::Context as _;
use sqlx::PgPool;
use super::settings::Settings;

/// Get the connection pool
pub async fn connection_pool(settings: Settings) -> anyhow::Result<PgPool> {
    let url = settings.database.url();
    // Create a connection pool
    let pool = PgPool::connect(&url).await?;
    // Create the tables if they don't exist
    // Run database migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to run migrations")?;
    Ok(pool)
}
