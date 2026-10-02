//! Settings that are not a page's content: the signed-in user's account,
//! and renaming an org. Destructive changes live in `deletion`.
//!
//! Deleting an org deletes what it holds outside Postgres first — every
//! connection's credential, every notebook secret and every compute
//! credential in the vault, and its members' compute schemas — so nothing
//! is orphaned where nobody can see it; then the org row, which cascades to
//! everything else.

use dioxus::prelude::ServerFnError;
use sqlx::Row;

use super::db::pool;
use super::errors::{bad_request, db_error, forbidden};
use crate::{validate_name, Account, Identity, Org, User};

pub async fn account(user: &User) -> Result<Account, ServerFnError> {
    let pool = pool()?;
    let identities = sqlx::query(
        "select issuer, \
         to_char(created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at, \
         to_char(last_login_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as last_login_at \
         from identities where user_id = $1::uuid order by created_at",
    )
    .bind(&user.id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?
    .iter()
    .map(|r| {
        let issuer: String = r.get("issuer");
        Identity {
            provider: match issuer.as_str() {
                "https://github.com" => "GitHub".to_string(),
                "https://accounts.google.com" => "Google".to_string(),
                other => other.to_string(),
            },
            created_at: r.get("created_at"),
            last_login_at: r.get("last_login_at"),
        }
    })
    .collect();
    let sessions: i64 = sqlx::query(
        "select count(*) as n from sessions where user_id = $1::uuid and expires_at > now()",
    )
    .bind(&user.id)
    .fetch_one(pool)
    .await
    .map_err(db_error)?
    .get("n");
    Ok(Account {
        user: user.clone(),
        identities,
        sessions,
        orgs: super::db::list_orgs_for(&user.id).await?,
    })
}

/// The name the user goes by; empty clears it (the email shows instead).
pub async fn set_display_name(user: &User, name: &str) -> Result<User, ServerFnError> {
    let name = name.trim();
    let name = if name.is_empty() {
        None
    } else {
        validate_name(name).map_err(bad_request)?;
        Some(name)
    };
    sqlx::query("update users set display_name = $2 where id = $1::uuid")
        .bind(&user.id)
        .bind(name)
        .execute(pool()?)
        .await
        .map_err(db_error)?;
    Ok(User {
        display_name: name.map(str::to_string),
        ..user.clone()
    })
}

/// End every session of the user but the one asking.
pub async fn sign_out_elsewhere(
    user: &User,
    current_hash: Option<Vec<u8>>,
) -> Result<u64, ServerFnError> {
    let done = sqlx::query(
        "delete from sessions where user_id = $1::uuid and ($2::bytea is null or id_hash <> $2)",
    )
    .bind(&user.id)
    .bind(current_hash)
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    Ok(done.rows_affected())
}

fn is_admin(org: &Org) -> bool {
    org.role == "owner" || org.role == "admin"
}

/// Rename an org (owners and admins). Its slug — its address — stays.
pub async fn rename_org(org: &Org, name: &str) -> Result<Org, ServerFnError> {
    if !is_admin(org) {
        return Err(forbidden("only the org's owners and admins can rename it"));
    }
    validate_name(name).map_err(bad_request)?;
    sqlx::query("update orgs set name = $2 where id = $1::uuid")
        .bind(&org.id)
        .bind(name.trim())
        .execute(pool()?)
        .await
        .map_err(db_error)?;
    Ok(Org {
        name: name.trim().to_string(),
        ..org.clone()
    })
}
