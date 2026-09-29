//! Connections: rows in `connections`, credentials in the vault, calls
//! through liaison (docs/connections.md §3, §7).

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
        Provider::GoogleCalendar => get(
            format!(
                "{}/calendar/v3/users/me/calendarList?maxResults=1",
                c.base_url
            ),
            json,
        ),
        Provider::MicrosoftCalendar => get(
            format!("{}/me/calendars?$top=1&$select=id,name", c.base_url),
            json,
        ),
        Provider::Gmail => get(format!("{}/gmail/v1/users/me/profile", c.base_url), json),
        Provider::Outlook => get(
            format!(
                "{}/me/mailFolders/inbox?$select=id,displayName,totalItemCount",
                c.base_url
            ),
            json,
        ),
        Provider::Notion => get(format!("{}/users/me", c.base_url), json),
        Provider::Jmap => get(
            c.external_id
                .clone()
                .unwrap_or_else(|| format!("{}/.well-known/jmap", c.base_url)),
            json,
        ),
        Provider::Caldav => ProviderCall {
            action: "read",
            method: "PROPFIND",
            url: format!("{}/", c.base_url),
            headers: vec![
                ("content-type", "application/xml; charset=utf-8"),
                ("depth", "1"),
            ],
            body: Some(
                concat!(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?>",
                    "<d:propfind xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">",
                    "<d:prop><d:displayname/><d:resourcetype/><c:calendar-home-set/></d:prop>",
                    "</d:propfind>"
                )
                .to_string(),
            ),
        },
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
    if c.provider == Provider::Caldav {
        return describe_caldav(body);
    }
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
        Provider::GoogleCalendar => {
            if json
                .as_ref()
                .and_then(|j| j.get("kind"))
                .and_then(Value::as_str)
                != Some("calendar#calendarList")
            {
                return Err("Google returned no calendar list".to_string());
            }
            Some("Google calendars are readable".to_string())
        }
        Provider::MicrosoftCalendar => {
            if !json
                .as_ref()
                .and_then(|j| j.get("value"))
                .is_some_and(Value::is_array)
            {
                return Err("Microsoft returned no calendar list".to_string());
            }
            Some("Microsoft calendars are readable".to_string())
        }
        Provider::Gmail => Some(format!(
            "Gmail of {}",
            field("/emailAddress").ok_or("Gmail returned no mailbox profile")?
        )),
        Provider::Outlook => Some(format!(
            "Outlook folder {}",
            field("/displayName").ok_or("Outlook returned no mailbox folder")?
        )),
        Provider::Notion => {
            if field("/object").as_deref() != Some("user") || field("/id").is_none() {
                return Err("Notion returned no token identity".to_string());
            }
            field("/bot/workspace_name")
                .map(|name| format!("connected to Notion workspace {name}"))
                .or_else(|| field("/name").map(|name| format!("connected to Notion as {name}")))
        }
        Provider::Jmap => {
            let session = json.as_ref().ok_or("the server returned no JMAP session")?;
            let accounts = session
                .get("accounts")
                .and_then(Value::as_object)
                .ok_or("the server returned no JMAP accounts")?;
            if !accounts.values().any(|a| {
                a.get("accountCapabilities")
                    .and_then(|v| v.get("urn:ietf:params:jmap:mail"))
                    .is_some_and(Value::is_object)
            }) {
                return Err("the JMAP token has no mail access".to_string());
            }
            let api_url = session
                .get("apiUrl")
                .and_then(Value::as_str)
                .ok_or("the JMAP session has no API URL")?;
            if !same_origin(&c.base_url, api_url) {
                return Err("the JMAP API is on another origin; this connection confines credentials to the session origin".to_string());
            }
            Some(
                field("/username")
                    .filter(|n| !n.is_empty())
                    .map(|n| format!("JMAP mail of {n}"))
                    .unwrap_or_else(|| "JMAP mail is accessible".to_string()),
            )
        }
        Provider::Caldav => unreachable!("CalDAV was described above"),
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

fn same_origin(base: &str, target: &str) -> bool {
    let (Ok(base), Ok(target)) = (url::Url::parse(base), url::Url::parse(target)) else {
        return false;
    };
    target.username().is_empty()
        && target.password().is_none()
        && target.fragment().is_none()
        && base.origin() == target.origin()
}

/// A 207 may contain only failed properties. Only successful DAV propstats
/// with a CalDAV collection or calendar home prove access; HTML is not success.
fn describe_caldav(body: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(body).map_err(|_| "the CalDAV reply is not UTF-8")?;
    let doc = roxmltree::Document::parse(text).map_err(|_| "the CalDAV reply is not valid XML")?;
    if !doc.root_element().has_tag_name(("DAV:", "multistatus")) {
        return Err("the server returned no CalDAV multistatus".to_string());
    }
    for propstat in doc
        .descendants()
        .filter(|n| n.has_tag_name(("DAV:", "propstat")))
    {
        let success = propstat
            .children()
            .find(|n| n.has_tag_name(("DAV:", "status")))
            .and_then(|n| n.text())
            .and_then(|s| s.split_whitespace().nth(1))
            == Some("200");
        if !success {
            continue;
        }
        let Some(prop) = propstat
            .children()
            .find(|n| n.has_tag_name(("DAV:", "prop")))
        else {
            continue;
        };
        let calendar = prop.children().any(|n| {
            (n.has_tag_name(("DAV:", "resourcetype"))
                && n.children()
                    .any(|c| c.has_tag_name(("urn:ietf:params:xml:ns:caldav", "calendar"))))
                || (n.has_tag_name(("urn:ietf:params:xml:ns:caldav", "calendar-home-set"))
                    && n.children().any(|c| {
                        c.has_tag_name(("DAV:", "href"))
                            && c.text().is_some_and(|s| !s.trim().is_empty())
                    }))
        });
        if calendar {
            let name = prop
                .children()
                .find(|n| n.has_tag_name(("DAV:", "displayname")))
                .and_then(|n| n.text());
            return Ok(name
                .map(|n| format!("CalDAV calendar {n}"))
                .unwrap_or_else(|| "CalDAV calendar access confirmed".to_string()));
        }
    }
    Err("the URL is not an accessible CalDAV calendar collection or calendar home".to_string())
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
            "calls to providers are disabled: the credential broker is not configured",
        ));
    }
    let root = warrant::root_key().map_err(unavailable)?;
    let provider = connection.provider.id();
    let grant =
        warrant::for_connection(&root, &org.id, provider, request.action, &connection.id, 0);
    let r = Request {
        now: grant.now,
        cost: 0,
        provider,
        action: request.action,
        resource: &connection.id,
        run_id: &grant.run_id,
        org_id: &org.id,
    };
    let minted = grant.warrant;
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
        bad_gateway("the credential broker is unreachable")
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
            "the credential broker refused the call ({status} {error})"
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
            message: format!("the credential broker refused the call ({status} {error})"),
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
            external_id: Some(if provider == Provider::Jmap {
                format!("{base_url}/jmap/session")
            } else {
                "+33612345678".into()
            }),
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
    fn productivity_probes_reject_unexpected_success_bodies() {
        for (provider, body, expected) in [
            (
                Provider::GoogleCalendar,
                br#"{"kind":"calendar#calendarList","items":[]}"#.as_slice(),
                "Google calendars are readable",
            ),
            (
                Provider::MicrosoftCalendar,
                br#"{"value":[]}"#.as_slice(),
                "Microsoft calendars are readable",
            ),
            (
                Provider::Gmail,
                br#"{"emailAddress":"me@example.com"}"#.as_slice(),
                "Gmail of me@example.com",
            ),
            (
                Provider::Outlook,
                br#"{"id":"inbox","displayName":"Inbox"}"#.as_slice(),
                "Outlook folder Inbox",
            ),
            (
                Provider::Notion,
                br#"{"object":"user","id":"bot","bot":{"workspace_name":"Acme"}}"#.as_slice(),
                "connected to Notion workspace Acme",
            ),
        ] {
            let c = connection(provider, provider.fixed_base_url().unwrap());
            assert_eq!(describe(&c, body).unwrap(), expected);
            assert!(describe(&c, b"<html>Sign in</html>").is_err());
            assert!(describe(&c, br#"{"error":"unauthorized"}"#).is_err());
        }
    }

    #[test]
    fn jmap_requires_mail_access_and_a_confined_api_endpoint() {
        let c = connection(Provider::Jmap, "https://api.fastmail.com");
        let session = |api_url: &str, capabilities: Value| {
            serde_json::to_vec(&serde_json::json!({
                "username": "me@example.com", "apiUrl": api_url,
                "accounts": {"a": {"accountCapabilities": capabilities}}
            }))
            .unwrap()
        };
        let mail = serde_json::json!({"urn:ietf:params:jmap:mail": {}});
        assert_eq!(
            describe(
                &c,
                &session("https://api.fastmail.com/jmap/api/", mail.clone())
            )
            .unwrap(),
            "JMAP mail of me@example.com"
        );
        assert!(describe(&c, &session("https://other.example.com/api", mail.clone())).is_err());
        assert!(describe(
            &c,
            &session(
                "https://api.fastmail.com@other.example.com/api",
                mail.clone()
            )
        )
        .is_err());
        assert!(describe(
            &c,
            &session("https://api.fastmail.com/api", serde_json::json!({}))
        )
        .is_err());
        assert!(describe(&c, b"not json").is_err());
    }

    #[test]
    fn caldav_checks_namespaces_and_each_propstat_status() {
        let reply = |status: &str, namespace: &str| {
            format!(
                "<d:multistatus xmlns:d=\"DAV:\" xmlns:c=\"{namespace}\"><d:response>\
             <d:propstat><d:prop><d:displayname>Team &amp; friends</d:displayname>\
             <d:resourcetype><d:collection/><c:calendar/></d:resourcetype></d:prop>\
             <d:status>HTTP/1.1 {status}</d:status></d:propstat></d:response></d:multistatus>"
            )
        };
        let namespace = "urn:ietf:params:xml:ns:caldav";
        assert_eq!(
            describe_caldav(reply("200 OK", namespace).as_bytes()).unwrap(),
            "CalDAV calendar Team & friends"
        );
        assert!(describe_caldav(reply("403 Forbidden", namespace).as_bytes()).is_err());
        assert!(describe_caldav(reply("200 OK", "wrong:namespace").as_bytes()).is_err());
        assert!(describe_caldav(b"<html>Login</html>").is_err());
        assert!(describe_caldav(br#"<d:multistatus xmlns:d="DAV:"/>"#).is_err());
        let c = connection(Provider::Caldav, "https://cloud.example.com/calendars/me");
        let call = probe(&c);
        assert_eq!(call.method, "PROPFIND");
        assert_eq!(call.url, "https://cloud.example.com/calendars/me/");
        assert!(call.headers.contains(&("depth", "1")));
        roxmltree::Document::parse(call.body.as_deref().unwrap()).unwrap();
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
