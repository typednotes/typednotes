use anyhow::Context as _;
use sqlx::PgPool;
use super::settings::Settings;

/// Get the connection pool
pub async fn connection_pool(settings: &Settings) -> anyhow::Result<PgPool> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_migration() {
        // Create the runtime
        let rt  = Runtime::new().unwrap();
        
        rt.block_on(async {
            let settings = Settings::new().unwrap_or_default();
            let url = settings.database.url();
            println!("URL = {url}");
            // Create a connection pool
            let pool = PgPool::connect(&url).await.expect("DB connection error");
            // Create the tables if they don't exist
            // Run database migrations
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .context("Failed to run migrations")
                .expect("DP migration error");
            }
        );
    }
}