//! # PostgreSQL connection pool — lazy singleton via `OnceCell`
//!
//! Implements the single-pool pattern for the TypedNotes server: a `static` [`OnceCell`]
//! holds the [`PgPool`] so that every server function, OAuth callback, and session store
//! shares the same set of connections without passing the pool through function arguments.
//!
//! ## Initialisation
//!
//! [`get_pool`] is the sole public entry point. On first invocation it:
//!
//! 1. Loads environment variables from `.env` via `dotenvy` (errors silently ignored so
//!    production deployments that inject env vars directly are unaffected).
//! 2. Reads `DATABASE_URL` — panics if unset, since no useful work can happen without a
//!    database.
//! 3. Opens a [`PgPoolOptions`] pool capped at **5 connections** and caches the resulting
//!    `PgPool` in the `OnceCell`.
//!
//! Subsequent calls return the cached pool immediately without re-connecting.
//!
//! ## Error handling
//!
//! Returns `Result<&'static PgPool, sqlx::Error>` so callers (typically server functions)
//! can convert the error into a `ServerFnError` and surface it to the client.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::OnceCell;

static POOL: OnceCell<PgPool> = OnceCell::const_new();

/// Get or initialize the database connection pool.
/// Uses DATABASE_URL environment variable for the connection string.
pub async fn get_pool() -> Result<&'static PgPool, sqlx::Error> {
    POOL.get_or_try_init(|| async {
        dotenvy::dotenv().ok();

        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set");

        PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
    })
    .await
}
