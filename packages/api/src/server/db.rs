//! The database pool, health, and orgs.
//!
//! The pool connects **lazily**: a Scaleway Serverless SQL Database sleeps
//! when idle, so connecting at startup could fail a perfectly healthy cold
//! start. The first query wakes it instead (sqlx retries within
//! `acquire_timeout`).
//!
//! Every org read is scoped to the caller's memberships: tenant isolation
//! rests on these queries (docs/services/core.md §7), so none of them selects
//! an org without joining `memberships` on the user.

use std::sync::OnceLock;
use std::time::Duration;

use dioxus::prelude::ServerFnError;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::Row;

use super::errors::{conflict, db_error, internal};
use super::{config, liaison, vault};
use crate::{Health, Org, SlugCheck};

static POOL: OnceLock<Result<PgPool, String>> = OnceLock::new();

/// The pool, built on first use from `DATABASE_URL`. A missing or unparsable
/// URL is reported on every request rather than panicking the server, so the
/// failure is visible in the UI and in `/api/health`.
pub fn pool() -> Result<&'static PgPool, ServerFnError> {
    POOL.get_or_init(|| {
        let url =
            std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is not set".to_string())?;
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

async fn table_exists(pool: &PgPool, table: &str) -> bool {
    sqlx::query("select to_regclass($1) is not null as present")
        .bind(table)
        .fetch_one(pool)
        .await
        .map(|row| row.get::<bool, _>("present"))
        .unwrap_or(false)
}

pub async fn health() -> Health {
    let configured = |database, schema, ledger| Health {
        database,
        schema,
        ledger,
        vault: vault::configured(),
        liaison: liaison::configured(),
        github: config::github().is_some(),
        google: config::google().is_some(),
        gitlab: config::gitlab().is_some(),
        dropbox: config::dropbox().is_some(),
        slack: config::slack().is_some(),
        slack_events: config::slack_signing_secret().is_some(),
        whatsapp_webhook: config::whatsapp_webhook().is_some(),
    };
    let Ok(pool) = pool() else {
        return configured(false, false, false);
    };
    let database = sqlx::query("select 1").execute(pool).await.is_ok();
    // `channel_messages` is the newest table of the app's own history.
    let schema = database
        && table_exists(pool, "public.orgs").await
        && table_exists(pool, "public.connections").await
        && table_exists(pool, "public.channel_messages").await;
    let ledger = database && table_exists(pool, "public.credit_ledger").await;
    configured(database, schema, ledger)
}

/// An org's columns plus the caller's role, from `orgs o join memberships m`.
/// A macro rather than a `const` so the queries below are built with
/// `concat!` — static strings, which sqlx 0.9 requires (it refuses
/// runtime-formatted SQL as an injection risk).
macro_rules! member_org_columns {
    () => {
        "o.id::text as id, o.slug::text as slug, o.name, m.role, \
         to_char(o.created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at"
    };
}

fn org_of(row: &PgRow) -> Org {
    Org {
        id: row.get("id"),
        slug: row.get("slug"),
        name: row.get("name"),
        role: row.get("role"),
        created_at: row.get("created_at"),
    }
}

/// The orgs `user_id` belongs to, newest first.
pub async fn list_orgs_for(user_id: &str) -> Result<Vec<Org>, ServerFnError> {
    let rows = sqlx::query(concat!(
        "select ",
        member_org_columns!(),
        " from orgs o join memberships m on m.org_id = o.id \
          where m.user_id = $1::uuid order by o.created_at desc"
    ))
    .bind(user_id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows.iter().map(org_of).collect())
}

/// The org called `slug`, if `user_id` is a member of it. Non-members get
/// `None`, exactly as for a slug that does not exist, so membership cannot be
/// probed.
pub async fn org_for_member(slug: &str, user_id: &str) -> Result<Option<Org>, ServerFnError> {
    let row = sqlx::query(concat!(
        "select ",
        member_org_columns!(),
        " from orgs o join memberships m on m.org_id = o.id \
          where o.slug = $1::citext and m.user_id = $2::uuid"
    ))
    .bind(slug)
    .bind(user_id)
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    Ok(row.as_ref().map(org_of))
}

/// [`org_for_member`], by org id (an OAuth flow records the id, not the slug).
pub async fn org_by_id_for_member(
    org_id: &str,
    user_id: &str,
) -> Result<Option<Org>, ServerFnError> {
    let row = sqlx::query(concat!(
        "select ",
        member_org_columns!(),
        " from orgs o join memberships m on m.org_id = o.id \
          where o.id = $1::uuid and m.user_id = $2::uuid"
    ))
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    Ok(row.as_ref().map(org_of))
}

/// Whether `slug` is free for a new org. Asked while the user types, so the
/// form says "taken" before submitting; creation still relies on the unique
/// index. Any signed-in user may ask: org slugs are public names, like
/// GitHub's.
pub async fn check_org_slug(slug: &str) -> Result<SlugCheck, ServerFnError> {
    if let Err(message) = crate::validate_slug(slug) {
        return Ok(SlugCheck {
            available: false,
            message,
        });
    }
    let taken = sqlx::query("select 1 from orgs where slug = $1::citext")
        .bind(slug)
        .fetch_optional(pool()?)
        .await
        .map_err(db_error)?
        .is_some();
    Ok(slug_check(slug, taken))
}

/// The answer for a well-formed slug.
pub fn slug_check(slug: &str, taken: bool) -> SlugCheck {
    if taken {
        SlugCheck {
            available: false,
            message: format!("'{slug}' is already taken"),
        }
    } else {
        SlugCheck {
            available: true,
            message: format!("'{slug}' is available"),
        }
    }
}

/// Create an org owned by `user_id`: the org and the owner membership in one
/// transaction, then `ledger`'s welcome grant.
pub async fn create_org_for(user_id: &str, slug: &str, name: &str) -> Result<Org, ServerFnError> {
    let mut tx = pool()?.begin().await.map_err(db_error)?;
    let inserted = sqlx::query(
        "insert into orgs (slug, name) values ($1, $2) \
         returning id::text as id, slug::text as slug, name, 'owner' as role, \
         to_char(created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at",
    )
    .bind(slug)
    .bind(name)
    .fetch_one(&mut *tx)
    .await;
    let org = match inserted {
        Ok(row) => org_of(&row),
        // The unique index on `slug` is the check — no read-then-write race.
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            return Err(conflict(format!("the slug '{slug}' is already taken")));
        }
        Err(e) => return Err(db_error(e)),
    };
    sqlx::query(
        "insert into memberships (org_id, user_id, role) values ($1::uuid, $2::uuid, 'owner')",
    )
    .bind(&org.id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;

    welcome_grant(&org.id).await;
    Ok(org)
}

/// `ledger`'s grant statement, verbatim from `Ledger/Sql/Grant.lean`
/// (`grantSql`, pinned by `LedgerTests/Ledger/Sql/GrantTest.lean`). This app
/// cannot import Lean, so the literal text is the contract; change both or
/// neither.
pub const GRANT_SQL: &str = "insert into credit_ledger (org_id, delta, reason, idempotency_key)
values ($1::uuid, $2, 'grant', $3)
on conflict (idempotency_key) do nothing";

/// `ledger`'s `welcomeKey`: one welcome grant per org, however often this runs.
pub fn welcome_key(org_id: &str) -> String {
    format!("welcome:{org_id}")
}

/// Grant a new org its welcome credits. Best-effort and after the org's own
/// transaction: the grant is idempotent, so a failure here (for instance
/// `ledger`'s history not applied yet) is logged and can be retried, while
/// the org itself exists either way.
async fn welcome_grant(org_id: &str) {
    let amount = config::welcome_credits();
    if amount == 0 {
        return;
    }
    let Ok(pool) = pool() else { return };
    if let Err(e) = sqlx::query(GRANT_SQL)
        .bind(org_id)
        .bind(amount)
        .bind(welcome_key(org_id))
        .execute(pool)
        .await
    {
        eprintln!("welcome grant for org {org_id} failed (is ledger's history applied?): {e}");
    }
}

/// The org's spendable credits: the ledger balance minus open holds — the
/// same quantity liaison's atomic reserve checks. `None` when `ledger`'s
/// tables are absent.
pub async fn available_credits(org_id: &str) -> Option<i64> {
    let pool = pool().ok()?;
    sqlx::query(
        "select ((select coalesce(sum(delta), 0) from credit_ledger where org_id = $1::uuid) \
               - (select coalesce(sum(amount), 0) from credit_holds \
                   where org_id = $1::uuid and state = 'held'))::bigint as available",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .map(|row| row.get::<i64, _>("available"))
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte `ledger`'s `grantSql`.
    #[test]
    fn grant_sql_is_ledgers() {
        assert_eq!(
            GRANT_SQL,
            "insert into credit_ledger (org_id, delta, reason, idempotency_key)\n\
             values ($1::uuid, $2, 'grant', $3)\n\
             on conflict (idempotency_key) do nothing"
        );
        assert_eq!(welcome_key("0b8c"), "welcome:0b8c");
    }
}
