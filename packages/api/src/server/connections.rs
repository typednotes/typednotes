//! Connections: rows in `connections`, credentials in the vault, tests
//! through liaison (docs/connections.md §3, §7).

use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::prelude::ServerFnError;
use serde_json::Value;
use sqlx::postgres::PgRow;
use sqlx::Row;

use super::db::pool;
use super::errors::{bad_gateway, db_error, forbidden, not_found, unavailable};
use super::liaison::{self, Call, Outcome, Request};
use super::{vault, warrant};
use crate::{Connection, Org, Provider, TestResult, User};

macro_rules! connection_columns {
    () => {
        "c.id::text as id, c.provider, c.label, c.base_url, c.status, c.last_error, \
         c.user_id::text as user_id, u.email::text as owner_email, \
         to_char(c.created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at, \
         to_char(c.last_checked_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as last_checked_at"
    };
}

/// Who may remove a connection: whoever made it, or the org's admins.
fn can_remove(org: &Org, user: &User, owner_id: &str) -> bool {
    owner_id == user.id || org.role == "owner" || org.role == "admin"
}

fn connection_of(row: &PgRow, org: &Org, user: &User) -> Connection {
    let provider: String = row.get("provider");
    let owner_id: String = row.get("user_id");
    Connection {
        id: row.get("id"),
        // The check constraint lists exactly `Provider::ALL`.
        provider: Provider::from_id(&provider).unwrap_or(Provider::OpenaiCompatible),
        label: row.get("label"),
        base_url: row.get("base_url"),
        status: row.get("status"),
        owner_email: row.get("owner_email"),
        created_at: row.get("created_at"),
        last_checked_at: row.get("last_checked_at"),
        last_error: row.get("last_error"),
        can_remove: can_remove(org, user, &owner_id),
    }
}

pub async fn list(org: &Org, user: &User) -> Result<Vec<Connection>, ServerFnError> {
    let rows = sqlx::query(concat!(
        "select ",
        connection_columns!(),
        " from connections c join users u on u.id = c.user_id \
          where c.org_id = $1::uuid order by c.created_at desc"
    ))
    .bind(&org.id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows.iter().map(|r| connection_of(r, org, user)).collect())
}

/// One connection of `org`, with its owner's id.
async fn get(org: &Org, user: &User, id: &str) -> Result<(Connection, String), ServerFnError> {
    let row = sqlx::query(concat!(
        "select ",
        connection_columns!(),
        " from connections c join users u on u.id = c.user_id \
          where c.org_id = $1::uuid and c.id::text = $2"
    ))
    .bind(&org.id)
    .bind(id)
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("no such connection"))?;
    let owner: String = row.get("user_id");
    Ok((connection_of(&row, org, user), owner))
}

/// Create a connection: a `pending` row (whose id names the vault path),
/// the credential into the vault, then `active`. If the vault refuses, the
/// row is removed again, so a listed connection always had its credential
/// stored.
pub async fn store(
    org: &Org,
    user: &User,
    provider: Provider,
    label: &str,
    base_url: &str,
    credential: Value,
) -> Result<Connection, ServerFnError> {
    if !vault::configured() {
        return Err(unavailable(
            "connections are disabled: the vault is not configured",
        ));
    }
    let pool = pool()?;
    let id: String = sqlx::query(
        "insert into connections (org_id, user_id, provider, label, base_url) \
         values ($1::uuid, $2::uuid, $3, $4, $5) returning id::text as id",
    )
    .bind(&org.id)
    .bind(&user.id)
    .bind(provider.id())
    .bind(label)
    .bind(base_url)
    .fetch_one(pool)
    .await
    .map_err(db_error)?
    .get("id");

    if let Err(e) = vault::write(
        &vault::credential_path(provider, &user.id, &id),
        &credential,
    )
    .await
    {
        eprintln!("vault write for connection {id} failed: {e}");
        let _ = sqlx::query("delete from connections where id = $1::uuid")
            .bind(&id)
            .execute(pool)
            .await;
        return Err(bad_gateway("could not store the credential in the vault"));
    }
    sqlx::query("update connections set status = 'active' where id = $1::uuid")
        .bind(&id)
        .execute(pool)
        .await
        .map_err(db_error)?;
    Ok(get(org, user, &id).await?.0)
}

/// Remove a connection: the credential first, so a failure leaves the row in
/// place to retry rather than an orphaned secret nobody can see.
pub async fn remove(org: &Org, user: &User, id: &str) -> Result<(), ServerFnError> {
    let (connection, owner) = get(org, user, id).await?;
    if !connection.can_remove {
        return Err(forbidden(
            "only its creator or an org admin can remove this connection",
        ));
    }
    vault::delete(&vault::credential_path(connection.provider, &owner, id))
        .await
        .map_err(|e| {
            eprintln!("vault delete for connection {id} failed: {e}");
            bad_gateway("could not delete the credential from the vault")
        })?;
    sqlx::query("delete from connections where id = $1::uuid")
        .bind(id)
        .execute(pool()?)
        .await
        .map_err(db_error)?;
    Ok(())
}

/// The cheap read-only call that proves a connection works (§7).
fn probe(c: &Connection) -> (String, Vec<(&'static str, &'static str)>) {
    match c.provider {
        Provider::Github => (
            "https://api.github.com/user".to_string(),
            vec![
                ("accept", "application/vnd.github+json"),
                ("user-agent", "typednotes"),
            ],
        ),
        Provider::Gdrive => (
            "https://www.googleapis.com/drive/v3/about?fields=user".to_string(),
            vec![("accept", "application/json")],
        ),
        Provider::S3 => (format!("{}?list-type=2&max-keys=1", c.base_url), vec![]),
        _ => (
            format!("{}/models", c.base_url),
            vec![("accept", "application/json")],
        ),
    }
}

/// A one-line description of a successful probe's answer.
fn describe(provider: Provider, body: &[u8]) -> String {
    let json: Option<Value> = serde_json::from_slice(body).ok();
    let field = |ptr: &str| {
        json.as_ref()
            .and_then(|j| j.pointer(ptr))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    match provider {
        Provider::Github => field("/login").map(|l| format!("signed in to GitHub as @{l}")),
        Provider::Gdrive => field("/user/emailAddress").map(|e| format!("Google Drive of {e}")),
        Provider::S3 => Some("bucket listed".to_string()),
        _ => json
            .as_ref()
            .and_then(|j| j.get("data"))
            .and_then(Value::as_array)
            .map(|models| format!("{} models available", models.len())),
    }
    .unwrap_or_else(|| "the provider answered".to_string())
}

/// Test a connection end to end: mint a narrow warrant for this one
/// connection, and have liaison — which checks it, holds zero credits,
/// fetches the credential and audits the call — read the provider.
pub async fn test(org: &Org, user: &User, id: &str) -> Result<TestResult, ServerFnError> {
    let (connection, owner) = get(org, user, id).await?;
    if !liaison::configured() {
        return Err(unavailable("tests are disabled: liaison is not configured"));
    }
    let root = warrant::root_key().map_err(unavailable)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let run_id = uuid::Uuid::new_v4().to_string();
    let provider = connection.provider.id();
    let minted = warrant::mint(
        &root,
        &uuid::Uuid::new_v4().to_string(),
        &org.id,
        vec![
            warrant::Caveat::ExpiresAt(now + 300),
            warrant::Caveat::Capability {
                provider: provider.to_string(),
                action: "read".to_string(),
            },
            warrant::Caveat::Resource(connection.id.clone()),
            warrant::Caveat::Budget(0),
            warrant::Caveat::RunId(run_id.clone()),
        ],
    );
    let (url, headers) = probe(&connection);
    let request = Request {
        now,
        cost: 0,
        provider,
        action: "read",
        resource: &connection.id,
        run_id: &run_id,
        org_id: &org.id,
    };
    let call = Call {
        account: format!("{owner}/{}", connection.id),
        method: "GET",
        url,
        headers,
    };

    let result = match liaison::egress(&minted, &request, &call).await {
        Err(e) => {
            eprintln!("liaison egress for connection {id} failed: {e}");
            return Err(bad_gateway("liaison is unreachable"));
        }
        Ok(Outcome::Upstream { status, body }) if (200..300).contains(&status) => TestResult {
            ok: true,
            message: describe(connection.provider, &body),
        },
        Ok(Outcome::Upstream { status, body }) => {
            let snippet: String = String::from_utf8_lossy(&body).chars().take(160).collect();
            TestResult {
                ok: false,
                message: format!("the provider answered {status}: {snippet}"),
            }
        }
        Ok(Outcome::Refused { status, error }) => TestResult {
            ok: false,
            message: format!("liaison refused the call ({status} {error})"),
        },
    };

    sqlx::query(
        "update connections set last_checked_at = now(), status = $2, last_error = $3 \
         where id = $1::uuid",
    )
    .bind(id)
    .bind(if result.ok { "active" } else { "failed" })
    .bind(if result.ok {
        None
    } else {
        Some(&result.message)
    })
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(provider: Provider, base_url: &str) -> Connection {
        Connection {
            id: "c".into(),
            provider,
            label: "l".into(),
            base_url: base_url.into(),
            status: "active".into(),
            owner_email: "a@b.c".into(),
            created_at: String::new(),
            last_checked_at: None,
            last_error: None,
            can_remove: true,
        }
    }

    #[test]
    fn probes_stay_under_base_url() {
        for p in Provider::ALL {
            let base = p
                .fixed_base_url()
                .unwrap_or("https://s3.example.com/bucket");
            let (url, _) = probe(&connection(p, base));
            assert!(url.starts_with(base), "{url} is outside {base}");
            let rest = &url[base.len()..];
            assert!(rest.starts_with('/') || rest.starts_with('?'), "{url}");
        }
    }

    #[test]
    fn describes_answers() {
        assert_eq!(
            describe(Provider::Github, br#"{"login":"octo"}"#),
            "signed in to GitHub as @octo"
        );
        assert_eq!(
            describe(Provider::Mistral, br#"{"data":[{},{}]}"#),
            "2 models available"
        );
        assert_eq!(describe(Provider::Gdrive, b"nope"), "the provider answered");
    }

    #[test]
    fn removal_rights() {
        let user = User {
            id: "u".into(),
            email: "e".into(),
            display_name: None,
        };
        let org = |role: &str| Org {
            id: "o".into(),
            slug: "s".into(),
            name: "n".into(),
            role: role.into(),
            created_at: String::new(),
        };
        assert!(can_remove(&org("member"), &user, "u"));
        assert!(!can_remove(&org("member"), &user, "other"));
        assert!(can_remove(&org("admin"), &user, "other"));
    }
}
