//! Server-side database access. Compiled only with the `server` feature.
//!
//! The pool connects **lazily**: a Scaleway Serverless SQL Database sleeps
//! when idle, so connecting at startup could fail a perfectly healthy cold
//! start. The first query wakes it instead (sqlx retries within
//! `acquire_timeout`).

use std::sync::OnceLock;
use std::time::Duration;

use dioxus::prelude::ServerFnError;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;

use crate::{Health, Org};

static POOL: OnceLock<Result<PgPool, String>> = OnceLock::new();

/// The pool, built on first use from `DATABASE_URL`. A missing or unparsable
/// URL is reported on every request rather than panicking the server, so the
/// failure is visible in the UI and in `/api/health`.
fn pool() -> Result<&'static PgPool, ServerFnError> {
    POOL.get_or_init(|| {
        let url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is not set".to_string())?;
        PgPoolOptions::new()
            .max_connections(5)
            // Long enough for a sleeping serverless database to wake.
            .acquire_timeout(Duration::from_secs(30))
            .connect_lazy(&url)
            .map_err(|e| format!("DATABASE_URL is not a valid connection string: {e}"))
    })
    .as_ref()
    .map_err(|message| internal(message.clone()))
}

fn internal(message: String) -> ServerFnError {
    ServerFnError::ServerError { message, code: 500, details: None }
}

fn db_error(e: sqlx::Error) -> ServerFnError {
    // Logged in full server-side; the client gets the message, which carries
    // no credentials (the URL never appears in sqlx's query errors).
    eprintln!("database error: {e}");
    internal(format!("database error: {e}"))
}

pub async fn health() -> Health {
    let Ok(pool) = pool() else {
        return Health { database: false, schema: false };
    };
    let database = sqlx::query("select 1").execute(pool).await.is_ok();
    let schema = database
        && sqlx::query("select to_regclass('public.orgs') is not null as present")
            .fetch_one(pool)
            .await
            .map(|row| row.get::<bool, _>("present"))
            .unwrap_or(false);
    Health { database, schema }
}

/// The `orgs` columns as the UI wants them. A macro rather than a `const` so
/// the queries below are built with `concat!` — static strings, which sqlx
/// 0.9 requires (it refuses runtime-formatted SQL as an injection risk).
macro_rules! org_columns {
    () => {
        "id::text as id, slug::text as slug, name, \
         to_char(created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at"
    };
}

fn org_of(row: &sqlx::postgres::PgRow) -> Org {
    Org {
        id: row.get("id"),
        slug: row.get("slug"),
        name: row.get("name"),
        created_at: row.get("created_at"),
    }
}

pub async fn list_orgs() -> Result<Vec<Org>, ServerFnError> {
    let rows = sqlx::query(concat!("select ", org_columns!(), " from orgs order by created_at desc"))
        .fetch_all(pool()?)
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(org_of).collect())
}

pub async fn create_org(slug: &str, name: &str) -> Result<Org, ServerFnError> {
    let result = sqlx::query(concat!(
        "insert into orgs (slug, name) values ($1, $2) returning ",
        org_columns!()
    ))
    .bind(slug)
    .bind(name)
    .fetch_one(pool()?)
    .await;
    match result {
        Ok(row) => Ok(org_of(&row)),
        // The unique index on `slug` is the check — no read-then-write race.
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            Err(ServerFnError::ServerError {
                message: format!("the slug '{slug}' is already taken"),
                code: 409,
                details: None,
            })
        }
        Err(e) => Err(db_error(e)),
    }
}
