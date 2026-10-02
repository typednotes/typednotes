//! Connections: rows in `connections`, credentials in the vault, calls
//! through liaison (docs/connections.md §3, §7).

use dioxus::prelude::ServerFnError;
use serde_json::Value;
use sqlx::postgres::PgRow;
use sqlx::Row;

use super::db::pool;
use super::errors::{bad_gateway, conflict, db_error, forbidden, not_found, unavailable};
use super::liaison::{self, Call, Outcome, Request};
use super::{connector, vault};
use crate::{Connection, Org, Provider, TestResult, User};

macro_rules! connection_columns {
    () => {
         "c.id::text as id, c.provider, c.label, c.base_url, c.external_id, c.status, c.last_error, c.permissions::text as permissions, \
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
        permissions: row.get::<Option<String>, _>("permissions").map(|s| serde_json::from_str(&s).unwrap_or(crate::ConnectorPermissions { scopes: Vec::new(), max_request_bytes: 1, max_response_bytes: 1 })),
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

/// A separate non-secret vault document lets the write-only app update policy
/// without retrieving or rewriting a connection's API key.
pub async fn set_permissions(org: &Org, user: &User, id: &str, permissions: &crate::ConnectorPermissions) -> Result<Connection, ServerFnError> {
    let (connection, owner) = get(org, user, id).await?;
    if !connection.can_remove { return Err(forbidden("only the connection's creator or an org admin can change its permissions")); }
    let permissions = permissions.validate(connection.provider).map_err(super::errors::bad_request)?;
    let mut tx = connector::lock(&org.id).await?;
    // Close the mandatory live gate before any distributed update. If either
    // vault or SQL fails, old warrants remain denied, never partially widened.
    connector::publish_ceiling(&org.id, connection.provider.id(), id, &crate::ConnectorPermissions::deny_all()).await?;
    connector::revoke(&mut tx, &org.id, Some(id)).await?;
    let path = format!("{}/permissions", vault::credential_path(connection.provider, &owner, id));
    vault::write(&path, &serde_json::to_value(&permissions).map_err(|_| super::errors::bad_request("invalid permissions"))?)
        .await.map_err(|_| bad_gateway("could not store the connection permissions"))?;
    sqlx::query("update connections set permissions = $2::jsonb where id = $1::uuid")
        .bind(id).bind(serde_json::to_string(&permissions).map_err(|_| super::errors::bad_request("invalid permissions"))?)
        .execute(&mut *tx).await.map_err(db_error)?;
    let organization = connector::ceiling(&connector::policy(&mut tx, &org.id).await?, &connection, &permissions);
    connector::publish_ceiling(&org.id, connection.provider.id(), id, &organization).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(get(org, user, id).await?.0)
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

pub async fn ensure_available(org: &Org, provider: Provider) -> Result<(), ServerFnError> {
    let existing = sqlx::query("select 1 from connections where org_id=$1::uuid and provider=$2 limit 1")
        .bind(&org.id).bind(provider.id()).fetch_optional(pool()?).await.map_err(db_error)?;
    if existing.is_some() { return Err(conflict(format!("{} is already connected to this organization; use or remove that connection first", provider.name()))); }
    Ok(())
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
    let mut tx = connector::lock(&org.id).await?;
    let guard = sqlx::query("select exists(select 1 from pg_trigger where tgrelid='public.connections'::regclass and tgname='connections_provider_once' and tgenabled in ('O','A')) as ready")
        .fetch_one(&mut *tx).await.map_err(db_error)?.get::<bool,_>("ready");
    if !guard { return Err(unavailable("connection creation requires migration 0009_connection_provider_guard.sql")); }
    connector::require_actor(&mut tx, &org.id, &user.id).await?;
    let existing = sqlx::query("select 1 from connections where org_id=$1::uuid and provider=$2 limit 1")
        .bind(&org.id).bind(new.provider.id()).fetch_optional(&mut *tx).await.map_err(db_error)?;
    if existing.is_some() { return Err(conflict(format!("{} is already connected to this organization; use or remove that connection first", new.provider.name()))); }
    let inserted = sqlx::query(
        "insert into connections (org_id, user_id, provider, label, base_url, external_id) \
         values ($1::uuid, $2::uuid, $3, $4, $5, $6) returning id::text as id",
    )
    .bind(&org.id)
    .bind(&user.id)
    .bind(new.provider.id())
    .bind(new.label)
    .bind(new.base_url)
    .bind(new.external_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| match error {
        sqlx::Error::Database(ref e) if e.constraint() == Some("connections_org_provider_once") => conflict(format!("{} is already connected to this organization", new.provider.name())),
        other => db_error(other),
    })?;
    let id: String = inserted.get("id");

    if let Err(e) = vault::write(
        &vault::credential_path(new.provider, &user.id, &id),
        &credential,
    )
    .await
    {
        eprintln!("vault write for connection {id} failed: {e}");
        let _ = vault::delete(&vault::credential_path(new.provider, &user.id, &id)).await;
        return Err(bad_gateway(format!(
            "could not store the credential in the vault: {e}"
        )));
    }
    let permissions = crate::ConnectorPermissions::preset(new.provider, crate::PermissionPreset::ReadOnly);
    let path = vault::credential_path(new.provider, &user.id, &id);
    if let Err(e) = vault::write(&format!("{path}/permissions"), &serde_json::json!(permissions)).await {
        let _ = vault::delete(&path).await;
        let _ = vault::delete(&format!("{path}/permissions")).await;
        return Err(bad_gateway(format!("could not provision connection permissions: {e}")));
    }
    let policy = connector::policy(&mut tx, &org.id).await?;
    let organization = if policy.allows_connector(new.provider) {
        policy.connector_ceilings.get(new.provider.id()).cloned().unwrap_or_else(|| permissions.clone())
    } else { crate::ConnectorPermissions::deny_all() };
    connector::publish_ceiling(&org.id, new.provider.id(), &id, &organization).await?;
    sqlx::query("update connections set status = 'active', permissions = $2::jsonb where id = $1::uuid")
        .bind(&id)
        .bind(serde_json::json!(permissions).to_string())
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
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
    let mut tx = connector::lock(&org.id).await?;
    connector::publish_ceiling(&org.id, connection.provider.id(), id, &crate::ConnectorPermissions::deny_all()).await?;
    connector::revoke(&mut tx, &org.id, Some(id)).await?;
    vault::delete(&vault::credential_path(connection.provider, &owner, id))
        .await
        .map_err(|e| {
            eprintln!("vault delete for connection {id} failed: {e}");
            bad_gateway("could not delete the credential from the vault")
        })?;
    sqlx::query("delete from connections where id = $1::uuid")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    vault::delete(&format!("{}/permissions", vault::credential_path(connection.provider, &owner, id)))
        .await.map_err(|_| bad_gateway("could not delete connection permissions"))?;
    tx.commit().await.map_err(db_error)?;
    Ok(())
}

/// One call through liaison on a connection's credential.
pub struct ProviderCall {
    pub operation: String,
    pub resource: Vec<String>,
    pub payload: Value,
    pub permissions: Option<crate::ConnectorPermissions>,
    pub cell_id: Option<String>,
}

impl ProviderCall {
    pub fn new(operation: &str, resource: Vec<String>, payload: Value) -> Self {
        Self { operation: operation.into(), resource, payload, permissions: None, cell_id: None }
    }
    pub fn with_permissions(mut self, permissions: crate::ConnectorPermissions) -> Self {
        self.permissions = Some(permissions);
        self
    }
    pub fn with_cell(mut self, cell_id: Option<&str>) -> Self {
        self.cell_id = cell_id.map(str::to_string);
        self
    }
}

/// A probe consumes an existing read grant. Send-only providers are never tested
/// by sending an unsolicited message or by restoring their unsupported inventory.
fn probe(c: &Connection) -> Result<ProviderCall, String> {
    let permissions = c.permissions.clone().unwrap_or_else(|| crate::ConnectorPermissions::preset(c.provider, crate::PermissionPreset::ReadOnly));
    let (operation, root) = match c.provider {
        p if p.is_ai() => ("models.list", Vec::new()),
        Provider::Github | Provider::Gitlab => ("repositories.list", Vec::new()),
        Provider::S3 | Provider::Azure => ("objects.list", Vec::new()),
        Provider::Gdrive => ("files.list", vec!["root".into()]),
        Provider::Dropbox => ("files.list", Vec::new()),
        Provider::GoogleCalendar | Provider::MicrosoftCalendar | Provider::Caldav => ("calendars.list", Vec::new()),
        Provider::Gmail | Provider::Outlook => ("mailboxes.list", vec!["me".into()]),
        Provider::Slack => ("channels.list", Vec::new()),
        Provider::Notion => ("pages.read", Vec::new()),
        Provider::Jmap => ("mailboxes.list", Vec::new()),
        Provider::Signal | Provider::Whatsapp => return Err("this provider supports sending only; no read-only broker probe is available".into()),
        _ => return Err("no supported native probe".into()),
    };
    let resource = if permissions.permits(operation, &root) && !matches!(c.provider, Provider::Notion | Provider::Jmap) { root }
        else { permissions.scopes.iter().find(|scope| scope.operation == operation && !scope.root.is_empty())
            .map(|scope| scope.root.clone()).ok_or("configure a resource-scoped read permission to test this connection")? };
    Ok(ProviderCall::new(operation, resource, serde_json::json!({})))
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
        Provider::Github | Provider::Gitlab => {
            let count = json.as_ref().and_then(Value::as_array).ok_or("the provider returned no repository inventory")?.len();
            Some(format!("{count} repositories visible"))
        },
        Provider::Gdrive => {
            let count = json.as_ref().and_then(|j| j.get("files")).and_then(Value::as_array).ok_or("Google returned no file inventory")?.len();
            Some(format!("{count} files visible"))
        },
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
        Provider::Gmail | Provider::Outlook => {
            let key = if c.provider == Provider::Gmail { "labels" } else { "value" };
            let count = json.as_ref().and_then(|j| j.get(key)).and_then(Value::as_array).ok_or("the provider returned no mailbox inventory")?.len();
            Some(format!("{count} mailboxes visible"))
        },
        Provider::Notion => {
            if field("/object").as_deref() != Some("page") || field("/id").is_none() {
                return Err("Notion returned no selected page".to_string());
            }
            Some("selected Notion page is readable".into())
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
        Provider::Dropbox => {
            let count = json.as_ref().and_then(|j| j.get("entries")).and_then(Value::as_array).ok_or("Dropbox returned no folder inventory")?.len();
            Some(format!("{count} files visible"))
        },
        Provider::S3 => Some("bucket listed".to_string()),
        Provider::Azure => Some("container listed".to_string()),
        Provider::Slack => {
            if let Some(error) = slack_error(body) {
                return Err(format!("Slack refused: {error}"));
            }
            let count = json.as_ref().and_then(|j| j.get("channels")).and_then(Value::as_array).ok_or("Slack returned no channel inventory")?.len();
            Some(format!("{count} channels visible"))
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
        p if p.is_ai() => {
            let json = json.as_ref().ok_or("the provider returned no JSON model catalog")?;
            if json.get("error").is_some_and(|e| !e.is_null()) {
                return Err("the provider returned an error instead of a model catalog".into());
            }
            let models = json
                .get("data")
                .or_else(|| json.get("models"))
                .or_else(|| (p == Provider::TypeSafe).then_some(json))
                .and_then(Value::as_array)
                .ok_or("the provider returned no model catalog")?;
            Some(format!("{} models available", models.len()))
        }
        _ => unreachable!("all connection providers have a description"),
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
    user: &User,
    connection: &Connection,
    owner: &str,
    request: ProviderCall,
) -> Result<Outcome, ServerFnError> {
    call_with_cost(org, user, connection, owner, request, 0).await
}

/// Budgeted inference uses the same confinement and broker as read-only probes.
pub async fn call_with_cost(
    org: &Org,
    user: &User,
    connection: &Connection,
    owner: &str,
    request: ProviderCall,
    cost: u64,
) -> Result<Outcome, ServerFnError> {
    if !liaison::configured() {
        return Err(unavailable(
            "calls to providers are disabled: the credential broker is not configured",
        ));
    }
    if !crate::ConnectorPermissions::valid_resource(&request.resource) || !request.payload.is_object() {
        return Err(super::errors::bad_request("invalid native connector resource or payload"));
    }
    let parent = connection.permissions.clone().unwrap_or_else(|| crate::ConnectorPermissions::preset(connection.provider, crate::PermissionPreset::ReadOnly));
    let requested = match request.permissions {
        Some(permissions) => parent.narrow(&permissions.validate(connection.provider).map_err(super::errors::bad_request)?).map_err(forbidden)?,
        None => parent,
    };
    if !requested.permits(&request.operation, &request.resource) {
        return Err(forbidden("the connection does not permit this native resource"));
    }
    let provider = connection.provider.id();
    let minted = connector::mint_for_cell(org, user, connection, owner, &request.operation, &requested, cost, request.cell_id.as_deref()).await?;
    if !minted.cell.permits(&request.operation, &request.resource)
        || request.payload.to_string().len() as u64 > minted.cell.max_request_bytes {
        return Err(forbidden("the live organization ceiling does not permit this native call"));
    }
    let grant = minted.grant;
    let r = Request {
        now: grant.now,
        cost,
        provider,
        action: &request.operation,
        resource: &connection.id,
        run_id: &grant.run_id,
        org_id: &org.id,
    };
    let minted = grant.warrant;
    let c = Call {
        account: format!("{owner}/{}", connection.id),
        operation: request.operation.clone(),
        resource: request.resource,
        payload: request.payload.to_string(),
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
    user: &User,
    connection: &Connection,
    owner: &str,
    request: ProviderCall,
) -> Result<Vec<u8>, ServerFnError> {
    match call(org, user, connection, owner, request).await? {
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
    let request = probe(&connection).map_err(super::errors::bad_request)?;
    let result = match call(org, user, &connection, &owner, request).await? {
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
            permissions: None,
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
    fn probes_are_native_read_operations_and_send_only_providers_have_none() {
        for &p in Provider::ALL {
            let base = p
                .fixed_base_url()
                .unwrap_or("https://storage.example.com/bucket");
            let call = probe(&connection(p, base));
            if matches!(p, Provider::Signal | Provider::Whatsapp | Provider::Notion | Provider::Jmap) {
                assert!(call.is_err());
                continue;
            }
            let call = call.unwrap();
            assert!(crate::connector_operations(p).iter().any(|op| op.id == call.operation && !op.write));
            assert_eq!(call.payload, serde_json::json!({}));
        }
    }

    #[test]
    fn describes_answers() {
        let c = |p| connection(p, "https://x");
        assert_eq!(
            describe(&c(Provider::Github), br#"[{"full_name":"octo/hello"}]"#).unwrap(),
            "1 repositories visible"
        );
        assert_eq!(
            describe(&c(Provider::Mistral), br#"{"data":[{},{}]}"#).unwrap(),
            "2 models available"
        );
        assert!(describe(&c(Provider::Gdrive), b"nope").is_err());
        assert_eq!(
            describe(&c(Provider::Slack), br#"{"ok":true,"channels":[]}"#).unwrap(),
            "0 channels visible"
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
                br#"{"labels":[]}"#.as_slice(),
                "0 mailboxes visible",
            ),
            (
                Provider::Outlook,
                br#"{"value":[]}"#.as_slice(),
                "0 mailboxes visible",
            ),
            (
                Provider::Notion,
                br#"{"object":"page","id":"page"}"#.as_slice(),
                "selected Notion page is readable",
            ),
        ] {
            let c = connection(provider, provider.fixed_base_url().unwrap());
            assert_eq!(describe(&c, body).unwrap(), expected);
            assert!(describe(&c, b"<html>Sign in</html>").is_err());
            assert!(describe(&c, br#"{"error":"unauthorized"}"#).is_err());
        }
    }

    #[test]
    fn ai_probes_require_a_model_catalog_not_an_error_or_login_page() {
        for &p in Provider::AI {
            let c = connection(p, "https://x/v1");
            for body in [
                b"<html>Sign in</html>".as_slice(),
                br#"{"error":"unauthorized"}"#,
                br#"{"error":{"message":"denied"},"data":[]}"#,
                br#"{"ok":true}"#,
            ] {
                assert!(describe(&c, body).is_err(), "{p:?} accepted {body:?}");
            }
            let body = if p == Provider::Radius {
                br#"{"baseUrl":"https://radius.pi.dev/v1","models":[]}"#.as_slice()
            } else if p == Provider::TypeSafe {
                br#"[{"id":"jev-latest"}]"#.as_slice()
            } else {
                br#"{"data":[]}"#.as_slice()
            };
            assert!(describe(&c, body).is_ok(), "{p:?} rejected its model catalog");
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
        let call = probe(&c).unwrap();
        assert_eq!(call.operation, "calendars.list");
        assert!(call.resource.is_empty());
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
