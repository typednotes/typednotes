//! Settings that are not a page's content: the signed-in user's account,
//! renaming and deleting an org, renaming a project.
//!
//! Deleting an org deletes what it holds outside Postgres first — every
//! connection's credential, every notebook secret and every compute
//! credential in the vault, and its members' compute schemas — so nothing
//! is orphaned where nobody can see it; then the org row, which cascades to
//! everything else.

use dioxus::prelude::ServerFnError;
use sqlx::Row;

use super::db::pool;
use super::errors::{bad_gateway, bad_request, db_error, forbidden};
use super::{compute, graphs, vault};
use crate::{validate_name, Account, Identity, Org, Provider, User};

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

/// Delete an org and everything in it (owners only), once `confirm` is its
/// slug.
pub async fn delete_org(org: &Org, confirm: &str) -> Result<(), ServerFnError> {
    if org.role != "owner" {
        return Err(forbidden("only the org's owners can delete it"));
    }
    if confirm.trim() != org.slug {
        return Err(bad_request(format!(
            "type the org's slug, {}, to confirm",
            org.slug
        )));
    }
    let pool = pool()?;
    let vault_error = |what: String| {
        move |e: String| {
            eprintln!(
                "deleting org {}: vault delete of {what} failed: {e}",
                org.id
            );
            bad_gateway(
                "could not delete the org's credentials from the vault; nothing was deleted",
            )
        }
    };
    for r in sqlx::query(
        "select id::text as id, user_id::text as user_id, provider from connections where org_id = $1::uuid",
    )
    .bind(&org.id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?
    {
        let (id, owner, provider): (String, String, String) =
            (r.get("id"), r.get("user_id"), r.get("provider"));
        let Some(provider) = Provider::from_id(&provider) else {
            continue;
        };
        let path = vault::credential_path(provider, &owner, &id);
        vault::delete(&path).await.map_err(vault_error(path))?;
    }
    for r in sqlx::query(
        "select g.id::text as graph_id, c.config ->> 'name' as name from graph_cells c \
         join graphs g on g.id = c.graph_id join projects p on p.id = g.project_id \
         where p.org_id = $1::uuid and c.variant = 'secret' and c.config ? 'set_at'",
    )
    .bind(&org.id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?
    {
        let (graph_id, name): (String, Option<String>) = (r.get("graph_id"), r.get("name"));
        if let Some(name) = name {
            let path = graphs::secret_path(&org.id, &graph_id, &name);
            vault::delete(&path).await.map_err(vault_error(path))?;
        }
    }
    let schemas: Vec<(String, String)> = sqlx::query(
        "select user_id::text as user_id, name from compute_schemas where org_id = $1::uuid",
    )
    .bind(&org.id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?
    .iter()
    .map(|r| (r.get("user_id"), r.get("name")))
    .collect();
    for (user_id, _) in &schemas {
        let path = compute::credential_path(&org.id, user_id);
        vault::delete(&path).await.map_err(vault_error(path))?;
    }
    for (_, name) in &schemas {
        if let Err(e) = compute::drop_schema(name).await {
            eprintln!(
                "deleting org {}: dropping compute schema {name} failed: {e}",
                org.id
            );
        }
    }
    sqlx::query("delete from orgs where id = $1::uuid")
        .bind(&org.id)
        .execute(pool)
        .await
        .map_err(db_error)?;
    Ok(())
}
