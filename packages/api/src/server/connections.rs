//! Connections: rows in `connections`, credentials in the vault, calls
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
        "c.id::text as id, c.provider, c.label, c.base_url, c.external_id, c.status, c.last_error, \
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
        external_id: row.get("external_id"),
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

/// One connection of `org`, with its owner's id (which names its vault path).
pub async fn get(org: &Org, user: &User, id: &str) -> Result<(Connection, String), ServerFnError> {
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

/// What a new connection is, apart from its credential.
pub struct NewConnection<'a> {
    pub provider: Provider,
    pub label: &'a str,
    pub base_url: &'a str,
    pub external_id: Option<&'a str>,
}

/// Create a connection: a `pending` row (whose id names the vault path),
/// the credential into the vault, then `active`. If the vault refuses, the
/// row is removed again, so a listed connection always had its credential
/// stored.
pub async fn store(
    org: &Org,
    user: &User,
    new: NewConnection<'_>,
    credential: Value,
) -> Result<Connection, ServerFnError> {
    if !vault::configured() {
        return Err(unavailable(
            "connections are disabled: the vault is not configured",
        ));
    }
    let pool = pool()?;
    let id: String = sqlx::query(
        "insert into connections (org_id, user_id, provider, label, base_url, external_id) \
         values ($1::uuid, $2::uuid, $3, $4, $5, $6) returning id::text as id",
    )
    .bind(&org.id)
    .bind(&user.id)
    .bind(new.provider.id())
    .bind(new.label)
    .bind(new.base_url)
    .bind(new.external_id)
    .fetch_one(pool)
    .await
    .map_err(db_error)?
    .get("id");

    if let Err(e) = vault::write(
        &vault::credential_path(new.provider, &user.id, &id),
        &credential,
    )
    .await
    {
        eprintln!("vault write for connection {id} failed: {e}");
        let _ = sqlx::query("delete from connections where id = $1::uuid")
            .bind(&id)
            .execute(pool)
            .await;
        return Err(bad_gateway(format!(
            "could not store the credential in the vault: {e}"
        )));
    }
    sqlx::query("update connections set status = 'active' where id = $1::uuid")
        .bind(&id)
        .execute(pool)
        .await
        .map_err(db_error)?;
    Ok(get(org, user, &id).await?.0)
}

/// Remove a connection: the credential first, so a failure leaves the row in
/// place to retry rather than an orphaned secret nobody can see. Channels
/// through it go with it (`on delete cascade`).
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

/// One call through liaison on a connection's credential.
pub struct ProviderCall<'a> {
    /// `read`, or `write` for calls that change something (sending a message).
    pub action: &'a str,
    pub method: &'a str,
    pub url: String,
    pub headers: Vec<(&'a str, &'a str)>,
    pub body: Option<String>,
}

/// The cheap read-only call that proves a connection works (§7).
fn probe(c: &Connection) -> ProviderCall<'static> {
    let get = |url: String, headers: Vec<(&'static str, &'static str)>| ProviderCall {
        action: "read",
        method: "GET",
        url,
        headers,
        body: None,
    };
    let json = vec![("accept", "application/json")];
    match c.provider {
        Provider::Github => get(
            "https://api.github.com/user".to_string(),
            vec![
                ("accept", "application/vnd.github+json"),
                ("user-agent", "typednotes"),
            ],
        ),
        Provider::Gitlab => get(format!("{}/user", c.base_url), json),
        Provider::Gdrive => get(
            "https://www.googleapis.com/drive/v3/about?fields=user".to_string(),
            json,
        ),
        Provider::Dropbox => ProviderCall {
            action: "read",
            method: "POST",
            url: format!("{}/2/users/get_current_account", c.base_url),
            headers: vec![("content-type", "application/json")],
            body: Some("null".to_string()),
        },
        Provider::S3 => get(format!("{}?list-type=2&max-keys=1", c.base_url), vec![]),
        Provider::Azure => get(
            format!("{}?restype=container&comp=list&maxresults=1", c.base_url),
            vec![],
        ),
        Provider::Slack => get(format!("{}/auth.test", c.base_url), json),
        Provider::Whatsapp => get(
            format!(
                "{}/{}?fields=display_phone_number,verified_name",
                c.base_url,
                c.external_id.as_deref().unwrap_or("me")
            ),
            json,
        ),
        Provider::Signal => get(format!("{}/v1/accounts", c.base_url), json),
        Provider::Mistral | Provider::Openai | Provider::Anthropic | Provider::OpenaiCompatible => {
            get(format!("{}/models", c.base_url), json)
        }
    }
}

/// Slack answers `200` with `{"ok": false, "error": …}` on failure.
pub fn slack_error(body: &[u8]) -> Option<String> {
    let json: Value = serde_json::from_slice(body).ok()?;
    if json.get("ok").and_then(Value::as_bool) == Some(true) {
        None
    } else {
        Some(
            json.get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string(),
        )
    }
}

/// A one-line description of a successful probe's answer, or why a `2xx`
/// answer is still a failure.
fn describe(c: &Connection, body: &[u8]) -> Result<String, String> {
    let json: Option<Value> = serde_json::from_slice(body).ok();
    let field = |ptr: &str| {
        json.as_ref()
            .and_then(|j| j.pointer(ptr))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let described = match c.provider {
        Provider::Github => field("/login").map(|l| format!("signed in to GitHub as @{l}")),
        Provider::Gitlab => field("/username").map(|l| format!("signed in to GitLab as @{l}")),
        Provider::Gdrive => field("/user/emailAddress").map(|e| format!("Google Drive of {e}")),
        Provider::Dropbox => field("/email").map(|e| format!("Dropbox of {e}")),
        Provider::S3 => Some("bucket listed".to_string()),
        Provider::Azure => Some("container listed".to_string()),
        Provider::Slack => {
            if let Some(error) = slack_error(body) {
                return Err(format!("Slack refused: {error}"));
            }
            field("/team").map(|t| format!("installed in the {t} workspace"))
        }
        Provider::Whatsapp => {
            field("/display_phone_number").map(|n| match field("/verified_name") {
                Some(name) => format!("WhatsApp number {n} ({name})"),
                None => format!("WhatsApp number {n}"),
            })
        }
        Provider::Signal => {
            let number = c.external_id.clone().unwrap_or_default();
            let registered = json
                .as_ref()
                .and_then(Value::as_array)
                .is_some_and(|numbers| numbers.iter().any(|n| n.as_str() == Some(&number)));
            if !registered {
                return Err(format!("the bridge has no account for {number}"));
            }
            Some(format!("the bridge sends as {number}"))
        }
        Provider::Mistral | Provider::Openai | Provider::Anthropic | Provider::OpenaiCompatible => {
            json.as_ref()
                .and_then(|j| j.get("data"))
                .and_then(Value::as_array)
                .map(|models| format!("{} models available", models.len()))
        }
    };
    Ok(described.unwrap_or_else(|| "the provider answered".to_string()))
}

/// Make one call on `connection`'s credential through liaison: mint a
/// narrow warrant for this one connection (5 minutes, zero budget, one
/// capability), and have liaison — which checks it, fetches the credential
/// and audits the call — make it. The app never sees the credential.
pub async fn call(
    org: &Org,
    connection: &Connection,
    owner: &str,
    request: ProviderCall<'_>,
) -> Result<Outcome, ServerFnError> {
    if !liaison::configured() {
        return Err(unavailable(
            "calls to providers are disabled: liaison is not configured",
        ));
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
                action: request.action.to_string(),
            },
            warrant::Caveat::Resource(connection.id.clone()),
            warrant::Caveat::Budget(0),
            warrant::Caveat::RunId(run_id.clone()),
        ],
    );
    let r = Request {
        now,
        cost: 0,
        provider,
        action: request.action,
        resource: &connection.id,
        run_id: &run_id,
        org_id: &org.id,
    };
    let c = Call {
        account: format!("{owner}/{}", connection.id),
        method: request.method,
        url: request.url,
        headers: request.headers,
        body: request.body,
    };
    liaison::egress(&minted, &r, &c).await.map_err(|e| {
        eprintln!(
            "liaison egress for connection {} failed: {e}",
            connection.id
        );
        bad_gateway("liaison is unreachable")
    })
}

/// [`call`], expecting a `2xx` answer: its body, or a message fit to show.
pub async fn call_ok(
    org: &Org,
    connection: &Connection,
    owner: &str,
    request: ProviderCall<'_>,
) -> Result<Vec<u8>, ServerFnError> {
    match call(org, connection, owner, request).await? {
        Outcome::Upstream { status, body } if (200..300).contains(&status) => Ok(body),
        Outcome::Upstream { status, body } => Err(bad_gateway(format!(
            "{} answered {status}: {}",
            connection.provider.name(),
            snippet(&body)
        ))),
        Outcome::Refused { status, error } => Err(bad_gateway(format!(
            "liaison refused the call ({status} {error})"
        ))),
    }
}

pub fn snippet(body: &[u8]) -> String {
    String::from_utf8_lossy(body).chars().take(160).collect()
}

/// Test a connection end to end with its probe, and record the outcome.
pub async fn test(org: &Org, user: &User, id: &str) -> Result<TestResult, ServerFnError> {
    let (connection, owner) = get(org, user, id).await?;
    let result = match call(org, &connection, &owner, probe(&connection)).await? {
        Outcome::Upstream { status, body } if (200..300).contains(&status) => {
            match describe(&connection, &body) {
                Ok(message) => TestResult { ok: true, message },
                Err(message) => TestResult { ok: false, message },
            }
        }
        Outcome::Upstream { status, body } => TestResult {
            ok: false,
            message: format!("the provider answered {status}: {}", snippet(&body)),
        },
        Outcome::Refused { status, error } => TestResult {
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
            external_id: Some("+33612345678".into()),
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
                .unwrap_or("https://storage.example.com/bucket");
            let call = probe(&connection(p, base));
            assert!(call.url.starts_with(base), "{} is outside {base}", call.url);
            let rest = &call.url[base.len()..];
            assert!(
                rest.starts_with('/') || rest.starts_with('?'),
                "{}",
                call.url
            );
            assert_eq!(call.action, "read");
        }
    }

    #[test]
    fn describes_answers() {
        let c = |p| connection(p, "https://x");
        assert_eq!(
            describe(&c(Provider::Github), br#"{"login":"octo"}"#).unwrap(),
            "signed in to GitHub as @octo"
        );
        assert_eq!(
            describe(&c(Provider::Mistral), br#"{"data":[{},{}]}"#).unwrap(),
            "2 models available"
        );
        assert_eq!(
            describe(&c(Provider::Gdrive), b"nope").unwrap(),
            "the provider answered"
        );
        assert_eq!(
            describe(&c(Provider::Slack), br#"{"ok":true,"team":"Acme"}"#).unwrap(),
            "installed in the Acme workspace"
        );
        assert!(describe(
            &c(Provider::Slack),
            br#"{"ok":false,"error":"invalid_auth"}"#
        )
        .unwrap_err()
        .contains("invalid_auth"));
        assert!(describe(&c(Provider::Signal), br#"["+33612345678"]"#).is_ok());
        assert!(describe(&c(Provider::Signal), br#"["+1555"]"#).is_err());
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
