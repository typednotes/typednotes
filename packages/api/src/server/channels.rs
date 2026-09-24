//! A project's messaging interfaces — Slack, WhatsApp, Signal — and its
//! inbox (docs/connections.md §11).
//!
//! A channel binds one inbound address to one project, through a connection
//! whose credential sits in the vault like any other:
//!
//! | Provider | Connection | Inbound address | Inbound delivery |
//! |---|---|---|---|
//! | `slack` | the app installed in a workspace (OAuth, bot token) | team id + channel id | Events API → `POST /hooks/slack` |
//! | `whatsapp` | Cloud API phone number id + access token | phone number id | webhook → `POST /hooks/whatsapp` |
//! | `signal` | a signal-cli-rest-api bridge + number + token | the number | pulled through liaison when the inbox loads |
//!
//! Webhook deliveries are authenticated by the providers' signatures
//! (Slack's signing secret, Meta's app secret) before anything is parsed.
//! Sends and pulls go through liaison with a one-connection warrant, like
//! every other provider call; the app never holds a channel's token.

use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::postgres::PgRow;
use sqlx::Row;

use dioxus::prelude::ServerFnError;

use super::connections::{self, NewConnection, ProviderCall};
use super::db::pool;
use super::errors::{bad_gateway, bad_request, conflict, db_error, not_found};
use super::projects;
use super::vault;
use crate::{
    validate_message, validate_phone, validate_signal, validate_whatsapp, Channel, Connection,
    Message, Org, Provider, SlackChannel, User,
};

// ── Channels ────────────────────────────────────────────────────────────

macro_rules! channel_columns {
    () => {
        "ch.id::text as id, ch.provider, ch.label, ch.connection_id::text as connection_id, \
         to_char(ch.created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at"
    };
}

fn channel_of(row: &PgRow) -> Channel {
    let provider: String = row.get("provider");
    Channel {
        id: row.get("id"),
        provider: Provider::from_id(&provider).unwrap_or(Provider::Slack),
        label: row.get("label"),
        connection_id: row.get("connection_id"),
        created_at: row.get("created_at"),
    }
}

pub async fn list(org: &Org, project_slug: &str) -> Result<Vec<Channel>, ServerFnError> {
    let (project, _) = projects::get(org, project_slug).await?;
    let rows = sqlx::query(concat!(
        "select ",
        channel_columns!(),
        " from channels ch where ch.project_id = $1::uuid order by ch.created_at"
    ))
    .bind(&project.id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows.iter().map(channel_of).collect())
}

/// One channel of the project, with the external address it routes.
struct Routed {
    channel: Channel,
    external_id: String,
    external_channel: Option<String>,
}

async fn get(org: &Org, project_slug: &str, channel_id: &str) -> Result<Routed, ServerFnError> {
    let (project, _) = projects::get(org, project_slug).await?;
    let row = sqlx::query(concat!(
        "select ",
        channel_columns!(),
        ", ch.external_id, ch.external_channel from channels ch \
          where ch.project_id = $1::uuid and ch.id::text = $2"
    ))
    .bind(&project.id)
    .bind(channel_id.trim())
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("no such interface"))?;
    Ok(Routed {
        channel: channel_of(&row),
        external_id: row.get("external_id"),
        external_channel: row.get("external_channel"),
    })
}

/// Bind an inbound address to the project. `409` if another channel (of any
/// project) already routes it.
async fn insert(
    project_id: &str,
    connection: &Connection,
    external_id: &str,
    external_channel: Option<&str>,
    label: &str,
) -> Result<Channel, ServerFnError> {
    let inserted = sqlx::query(concat!(
        "insert into channels as ch \
         (project_id, connection_id, provider, external_id, external_channel, label) \
         values ($1::uuid, $2::uuid, $3, $4, $5, $6) returning ",
        channel_columns!()
    ))
    .bind(project_id)
    .bind(&connection.id)
    .bind(connection.provider.id())
    .bind(external_id)
    .bind(external_channel)
    .bind(label)
    .fetch_one(pool()?)
    .await;
    match inserted {
        Ok(row) => Ok(channel_of(&row)),
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => Err(conflict(
            "this address is already the interface of a project",
        )),
        Err(e) => Err(db_error(e)),
    }
}

pub async fn remove(org: &Org, project_slug: &str, channel_id: &str) -> Result<(), ServerFnError> {
    let routed = get(org, project_slug, channel_id).await?;
    sqlx::query("delete from channels where id = $1::uuid")
        .bind(&routed.channel.id)
        .execute(pool()?)
        .await
        .map_err(db_error)?;
    Ok(())
}

/// A channel connection of `org` for `provider`.
async fn channel_connection(
    org: &Org,
    user: &User,
    connection_id: &str,
    provider: Provider,
) -> Result<(Connection, String), ServerFnError> {
    let (connection, owner) = connections::get(org, user, connection_id.trim()).await?;
    if connection.provider != provider {
        return Err(bad_request(format!("not a {} connection", provider.name())));
    }
    Ok((connection, owner))
}

// ── Slack ───────────────────────────────────────────────────────────────

/// Parse `conversations.list`.
fn parse_slack_channels(body: &[u8]) -> Result<Vec<SlackChannel>, String> {
    if let Some(error) = connections::slack_error(body) {
        return Err(format!("Slack refused: {error}"));
    }
    let json: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    let mut channels: Vec<SlackChannel> = json
        .get("channels")
        .and_then(Value::as_array)
        .map(|cs| {
            cs.iter()
                .filter_map(|c| {
                    Some(SlackChannel {
                        id: c.get("id")?.as_str()?.to_string(),
                        name: c.get("name")?.as_str()?.to_string(),
                        private: c
                            .get("is_private")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    channels.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(channels)
}

/// The channels the workspace's bot can post to (public ones, and private
/// ones it was invited to), by name.
pub async fn slack_channels(
    org: &Org,
    user: &User,
    connection_id: &str,
) -> Result<Vec<SlackChannel>, ServerFnError> {
    let (connection, owner) = channel_connection(org, user, connection_id, Provider::Slack).await?;
    let body = connections::call_ok(
        org,
        &connection,
        &owner,
        ProviderCall {
            action: "read",
            method: "GET",
            url: format!(
                "{}/conversations.list?types=public_channel,private_channel&exclude_archived=true&limit=200",
                connection.base_url
            ),
            headers: vec![("accept", "application/json")],
            body: None,
        },
    )
    .await?;
    parse_slack_channels(&body).map_err(bad_gateway)
}

pub async fn add_slack(
    org: &Org,
    user: &User,
    project_slug: &str,
    connection_id: &str,
    slack_channel_id: &str,
) -> Result<Channel, ServerFnError> {
    let (project, _) = projects::get(org, project_slug).await?;
    let (connection, _) = channel_connection(org, user, connection_id, Provider::Slack).await?;
    let team = connection
        .external_id
        .clone()
        .ok_or_else(|| bad_request("this Slack connection has no workspace id; reconnect it"))?;
    let channels = slack_channels(org, user, connection_id).await?;
    let chosen = channels
        .into_iter()
        .find(|c| c.id == slack_channel_id.trim())
        .ok_or_else(|| bad_request("the bot cannot see that Slack channel"))?;
    let label = format!("#{} · {}", chosen.name, connection.label);
    insert(&project.id, &connection, &team, Some(&chosen.id), &label).await
}

// ── WhatsApp and Signal: a connection and its channel in one step ──────

/// Store `connection`, bind it to the project, and undo the connection if
/// the address is taken — so a refused interface leaves no orphan.
async fn connect_and_bind(
    org: &Org,
    user: &User,
    project_id: &str,
    new: NewConnection<'_>,
    credential: Value,
) -> Result<Channel, ServerFnError> {
    let external_id = new.external_id.unwrap_or_default().to_string();
    let label = new.label.to_string();
    let connection = connections::store(org, user, new, credential).await?;
    match insert(project_id, &connection, &external_id, None, &label).await {
        Ok(channel) => Ok(channel),
        Err(e) => {
            if let Err(undo) = connections::remove(org, user, &connection.id).await {
                eprintln!("could not undo connection {}: {undo}", connection.id);
            }
            Err(e)
        }
    }
}

pub async fn connect_whatsapp(
    org: &Org,
    user: &User,
    project_slug: &str,
    phone_number_id: &str,
    access_token: &str,
) -> Result<Channel, ServerFnError> {
    let (project, _) = projects::get(org, project_slug).await?;
    let (id, token) = validate_whatsapp(phone_number_id, access_token).map_err(bad_request)?;
    let base_url = Provider::Whatsapp.fixed_base_url().unwrap_or_default();
    let label = format!("WhatsApp {id}");
    connect_and_bind(
        org,
        user,
        &project.id,
        NewConnection {
            provider: Provider::Whatsapp,
            label: &label,
            base_url,
            external_id: Some(&id),
        },
        vault::bearer(base_url, &token),
    )
    .await
}

pub async fn connect_signal(
    org: &Org,
    user: &User,
    project_slug: &str,
    base_url: &str,
    number: &str,
    token: &str,
) -> Result<Channel, ServerFnError> {
    let (project, _) = projects::get(org, project_slug).await?;
    let (base_url, number, token) =
        validate_signal(base_url, number, token).map_err(bad_request)?;
    let label = format!("Signal {number}");
    connect_and_bind(
        org,
        user,
        &project.id,
        NewConnection {
            provider: Provider::Signal,
            label: &label,
            base_url: &base_url,
            external_id: Some(&number),
        },
        vault::bearer(&base_url, &token),
    )
    .await
}

// ── Messages ────────────────────────────────────────────────────────────

macro_rules! message_columns {
    () => {
        "m.id::text as id, m.channel_id::text as channel_id, ch.provider, ch.label as channel_label, \
         m.direction, m.peer, m.peer_name, m.body, \
         to_char(m.created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at"
    };
}

fn message_of(row: &PgRow) -> Message {
    let provider: String = row.get("provider");
    Message {
        id: row.get("id"),
        channel_id: row.get("channel_id"),
        provider: Provider::from_id(&provider).unwrap_or(Provider::Slack),
        channel_label: row.get("channel_label"),
        direction: row.get("direction"),
        peer: row.get("peer"),
        peer_name: row.get("peer_name"),
        body: row.get("body"),
        created_at: row.get("created_at"),
    }
}

/// A message to record. `external_id` makes it idempotent per channel and
/// direction: a retried webhook or a repeated pull records nothing new.
pub struct NewMessage<'a> {
    pub channel_id: &'a str,
    pub direction: &'a str,
    pub external_id: Option<&'a str>,
    pub peer: &'a str,
    pub peer_name: Option<&'a str>,
    pub body: &'a str,
    pub sent_by: Option<&'a str>,
}

/// Record a message; `None` if it was already recorded.
async fn record(m: NewMessage<'_>) -> Result<Option<String>, ServerFnError> {
    let row = sqlx::query(
        "insert into channel_messages \
         (channel_id, direction, external_id, peer, peer_name, body, sent_by) \
         values ($1::uuid, $2, $3, $4, $5, $6, $7::uuid) \
         on conflict (channel_id, direction, external_id) do nothing returning id::text as id",
    )
    .bind(m.channel_id)
    .bind(m.direction)
    .bind(m.external_id)
    .bind(m.peer)
    .bind(m.peer_name)
    .bind(m.body)
    .bind(m.sent_by)
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    Ok(row.map(|r| r.get("id")))
}

/// The project's latest messages, newest first — after pulling what its
/// Signal bridges hold (best-effort: a bridge that is down only delays its
/// messages).
pub async fn inbox(
    org: &Org,
    user: &User,
    project_slug: &str,
) -> Result<Vec<Message>, ServerFnError> {
    let (project, _) = projects::get(org, project_slug).await?;
    for channel in list(org, project_slug).await? {
        if channel.provider == Provider::Signal {
            if let Err(e) = pull_signal(org, user, project_slug, &channel.id).await {
                eprintln!("signal pull for channel {} failed: {e}", channel.id);
            }
        }
    }
    let rows = sqlx::query(concat!(
        "select ",
        message_columns!(),
        " from channel_messages m join channels ch on ch.id = m.channel_id \
          where ch.project_id = $1::uuid order by m.created_at desc limit 100"
    ))
    .bind(&project.id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows.iter().map(message_of).collect())
}

async fn message_by_id(id: &str) -> Result<Message, ServerFnError> {
    let row = sqlx::query(concat!(
        "select ",
        message_columns!(),
        " from channel_messages m join channels ch on ch.id = m.channel_id where m.id = $1::uuid"
    ))
    .bind(id)
    .fetch_one(pool()?)
    .await
    .map_err(db_error)?;
    Ok(message_of(&row))
}

/// The provider call that sends `text`, and where it goes (the peer to
/// record). Slack posts to the bound channel; WhatsApp and Signal to
/// `recipient`.
fn send_call(
    routed: &Routed,
    connection: &Connection,
    recipient: &str,
    text: &str,
) -> Result<(ProviderCall<'static>, String), String> {
    let post = |url: String, body: Value| ProviderCall {
        action: "write",
        method: "POST",
        url,
        headers: vec![("content-type", "application/json; charset=utf-8")],
        body: Some(body.to_string()),
    };
    match connection.provider {
        Provider::Slack => {
            let channel = routed
                .external_channel
                .clone()
                .ok_or("this Slack interface has no channel")?;
            Ok((
                post(
                    format!("{}/chat.postMessage", connection.base_url),
                    json!({ "channel": channel, "text": text }),
                ),
                channel,
            ))
        }
        Provider::Whatsapp => {
            let to = validate_phone(recipient)?;
            Ok((
                post(
                    format!("{}/{}/messages", connection.base_url, routed.external_id),
                    json!({ "messaging_product": "whatsapp", "recipient_type": "individual",
                            "to": to, "type": "text", "text": { "body": text } }),
                ),
                to,
            ))
        }
        Provider::Signal => {
            let to = format!("+{}", validate_phone(recipient)?);
            Ok((
                post(
                    format!("{}/v2/send", connection.base_url),
                    json!({ "message": text, "number": routed.external_id, "recipients": [to] }),
                ),
                to,
            ))
        }
        _ => Err("not a messaging interface".to_string()),
    }
}

/// The provider's id for a sent message, from its answer.
fn sent_id(provider: Provider, body: &[u8]) -> Result<Option<String>, String> {
    let json: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    match provider {
        Provider::Slack => {
            if let Some(error) = connections::slack_error(body) {
                return Err(format!("Slack refused: {error}"));
            }
            Ok(json.get("ts").and_then(Value::as_str).map(str::to_string))
        }
        Provider::Whatsapp => Ok(json
            .pointer("/messages/0/id")
            .and_then(Value::as_str)
            .map(str::to_string)),
        Provider::Signal => Ok(json.get("timestamp").map(|t| match t {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })),
        _ => Ok(None),
    }
}

pub async fn send(
    org: &Org,
    user: &User,
    project_slug: &str,
    channel_id: &str,
    recipient: &str,
    text: &str,
) -> Result<Message, ServerFnError> {
    let text = validate_message(text).map_err(bad_request)?;
    let routed = get(org, project_slug, channel_id).await?;
    let (connection, owner) = connections::get(org, user, &routed.channel.connection_id).await?;
    let (call, peer) = send_call(&routed, &connection, recipient, &text).map_err(bad_request)?;
    let body = connections::call_ok(org, &connection, &owner, call).await?;
    let external_id = sent_id(connection.provider, &body).map_err(bad_gateway)?;
    let id = record(NewMessage {
        channel_id: &routed.channel.id,
        direction: "out",
        external_id: external_id.as_deref(),
        peer: &peer,
        peer_name: None,
        body: &text,
        sent_by: Some(&user.id),
    })
    .await?
    .ok_or_else(|| conflict("this message was already recorded"))?;
    message_by_id(&id).await
}

// ── Signal: pull ────────────────────────────────────────────────────────

/// An inbound message, as parsed from a provider's delivery.
#[derive(Clone, Debug, PartialEq)]
pub struct Inbound {
    pub external_id: String,
    pub peer: String,
    pub peer_name: Option<String>,
    pub body: String,
}

/// signal-cli-rest-api's `GET /v1/receive/{number}`: an array of envelopes;
/// only data messages with text count.
fn parse_signal(body: &[u8]) -> Result<Vec<Inbound>, String> {
    let json: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    let envelopes = json.as_array().ok_or("the bridge's answer is not a list")?;
    Ok(envelopes
        .iter()
        .filter_map(|item| {
            let envelope = item.get("envelope")?;
            let text = envelope.pointer("/dataMessage/message")?.as_str()?;
            let peer = envelope
                .get("sourceNumber")
                .and_then(Value::as_str)
                .or_else(|| envelope.get("source").and_then(Value::as_str))?;
            let timestamp = envelope.get("timestamp")?.as_u64()?;
            Some(Inbound {
                external_id: format!("{peer}:{timestamp}"),
                peer: peer.to_string(),
                peer_name: envelope
                    .get("sourceName")
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty())
                    .map(str::to_string),
                body: text.to_string(),
            })
        })
        .collect())
}

/// Pull what the Signal bridge holds for the channel's number into the
/// inbox. The bridge hands each message out once, so they are recorded as
/// they arrive.
async fn pull_signal(
    org: &Org,
    user: &User,
    project_slug: &str,
    channel_id: &str,
) -> Result<usize, ServerFnError> {
    let routed = get(org, project_slug, channel_id).await?;
    let (connection, owner) = connections::get(org, user, &routed.channel.connection_id).await?;
    let body = connections::call_ok(
        org,
        &connection,
        &owner,
        ProviderCall {
            action: "read",
            method: "GET",
            url: format!(
                "{}/v1/receive/{}",
                connection.base_url,
                routed.external_id.replace('+', "%2B")
            ),
            headers: vec![("accept", "application/json")],
            body: None,
        },
    )
    .await?;
    let mut recorded = 0;
    for m in parse_signal(&body).map_err(bad_gateway)? {
        if record(NewMessage {
            channel_id: &routed.channel.id,
            direction: "in",
            external_id: Some(&m.external_id),
            peer: &m.peer,
            peer_name: m.peer_name.as_deref(),
            body: &m.body,
            sent_by: None,
        })
        .await?
        .is_some()
        {
            recorded += 1;
        }
    }
    Ok(recorded)
}

// ── Webhooks: signatures ────────────────────────────────────────────────

fn hmac_hex_matches(secret: &str, message: &[&[u8]], hex_signature: &str) -> bool {
    let Ok(expected) = hex::decode(hex_signature) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    for part in message {
        mac.update(part);
    }
    // Constant-time comparison.
    mac.verify_slice(&expected).is_ok()
}

/// Slack's request signature: `v0=hex(HMAC-SHA256(secret, "v0:{ts}:{body}"))`,
/// with a timestamp at most five minutes from `now` (replay window).
pub fn slack_signature_ok(
    secret: &str,
    timestamp: &str,
    body: &[u8],
    signature: &str,
    now: u64,
) -> bool {
    let Ok(ts) = timestamp.parse::<u64>() else {
        return false;
    };
    if ts.abs_diff(now) > 300 {
        return false;
    }
    let Some(hex_sig) = signature.strip_prefix("v0=") else {
        return false;
    };
    hmac_hex_matches(secret, &[b"v0:", timestamp.as_bytes(), b":", body], hex_sig)
}

/// Meta's webhook signature: `sha256=hex(HMAC-SHA256(app_secret, body))`.
pub fn whatsapp_signature_ok(app_secret: &str, body: &[u8], signature: &str) -> bool {
    match signature.strip_prefix("sha256=") {
        Some(hex_sig) => hmac_hex_matches(app_secret, &[body], hex_sig),
        None => false,
    }
}

// ── Webhooks: payloads ──────────────────────────────────────────────────

/// What a Slack Events API delivery asks for.
#[derive(Debug, PartialEq)]
pub enum SlackEvent {
    /// Registering the request URL: answer with the challenge.
    UrlVerification(String),
    /// A person's message in a channel of a workspace.
    Message {
        team: String,
        channel: String,
        message: Inbound,
    },
    /// Anything else: bots (including this app's own posts), edits,
    /// joins… acknowledged and dropped.
    Ignored,
}

pub fn parse_slack_event(body: &[u8]) -> Result<SlackEvent, String> {
    let json: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    let str_at = |ptr: &str| json.pointer(ptr).and_then(Value::as_str);
    match str_at("/type") {
        Some("url_verification") => Ok(SlackEvent::UrlVerification(
            str_at("/challenge").ok_or("no challenge")?.to_string(),
        )),
        Some("event_callback") => {
            let event = json.get("event").ok_or("no event")?;
            let is_person_message = event.get("type").and_then(Value::as_str) == Some("message")
                && event.get("subtype").is_none()
                && event.get("bot_id").is_none();
            let fields = (
                str_at("/team_id"),
                event.get("channel").and_then(Value::as_str),
                event.get("user").and_then(Value::as_str),
                event.get("text").and_then(Value::as_str),
                event.get("ts").and_then(Value::as_str),
            );
            match fields {
                (Some(team), Some(channel), Some(user), Some(text), Some(ts))
                    if is_person_message =>
                {
                    Ok(SlackEvent::Message {
                        team: team.to_string(),
                        channel: channel.to_string(),
                        message: Inbound {
                            external_id: ts.to_string(),
                            peer: user.to_string(),
                            peer_name: None,
                            body: text.to_string(),
                        },
                    })
                }
                _ => Ok(SlackEvent::Ignored),
            }
        }
        _ => Ok(SlackEvent::Ignored),
    }
}

/// The messages of a WhatsApp webhook delivery, each with the phone number
/// id it was sent to. Non-text messages are recorded as `[image]`,
/// `[audio]`… so the inbox shows that something arrived.
pub fn parse_whatsapp(body: &[u8]) -> Result<Vec<(String, Inbound)>, String> {
    let json: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let entries = json
        .get("entry")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for change in entries
        .iter()
        .filter_map(|e| e.get("changes").and_then(Value::as_array))
        .flatten()
    {
        let Some(value) = change.get("value") else {
            continue;
        };
        let Some(phone_id) = value
            .pointer("/metadata/phone_number_id")
            .and_then(Value::as_str)
        else {
            continue;
        };
        let contacts = value.get("contacts").and_then(Value::as_array);
        let name_of = |wa_id: &str| {
            contacts?
                .iter()
                .find(|c| c.get("wa_id").and_then(Value::as_str) == Some(wa_id))?
                .pointer("/profile/name")?
                .as_str()
                .map(str::to_string)
        };
        for m in value
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(from), Some(id)) = (
                m.get("from").and_then(Value::as_str),
                m.get("id").and_then(Value::as_str),
            ) else {
                continue;
            };
            let kind = m.get("type").and_then(Value::as_str).unwrap_or("unknown");
            let body = match m.pointer("/text/body").and_then(Value::as_str) {
                Some(text) => text.to_string(),
                None => format!("[{kind}]"),
            };
            out.push((
                phone_id.to_string(),
                Inbound {
                    external_id: id.to_string(),
                    peer: from.to_string(),
                    peer_name: name_of(from),
                    body,
                },
            ));
        }
    }
    Ok(out)
}

/// The channel routing an inbound address, if any.
async fn route(
    provider: Provider,
    external_id: &str,
    external_channel: Option<&str>,
) -> Result<Option<String>, ServerFnError> {
    let row = sqlx::query(
        "select id::text as id from channels \
         where provider = $1 and external_id = $2 \
           and external_channel is not distinct from $3",
    )
    .bind(provider.id())
    .bind(external_id)
    .bind(external_channel)
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    Ok(row.map(|r| r.get("id")))
}

/// Record an inbound message for whichever project routes its address.
/// Messages to an address no project routes are dropped.
pub async fn deliver(
    provider: Provider,
    external_id: &str,
    external_channel: Option<&str>,
    m: &Inbound,
) -> Result<bool, ServerFnError> {
    let Some(channel_id) = route(provider, external_id, external_channel).await? else {
        return Ok(false);
    };
    Ok(record(NewMessage {
        channel_id: &channel_id,
        direction: "in",
        external_id: Some(&m.external_id),
        peer: &m.peer,
        peer_name: m.peer_name.as_deref(),
        body: &m.body,
        sent_by: None,
    })
    .await?
    .is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, parts: &[&[u8]]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        for p in parts {
            mac.update(p);
        }
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn slack_signatures() {
        let body = br#"{"type":"event_callback"}"#;
        let sig = format!("v0={}", sign("s3cret", &[b"v0:1790000000:", body]));
        assert!(slack_signature_ok(
            "s3cret",
            "1790000000",
            body,
            &sig,
            1_790_000_100
        ));
        // Too old, wrong secret, tampered body, malformed header.
        assert!(!slack_signature_ok(
            "s3cret",
            "1790000000",
            body,
            &sig,
            1_790_000_301
        ));
        assert!(!slack_signature_ok(
            "other",
            "1790000000",
            body,
            &sig,
            1_790_000_000
        ));
        assert!(!slack_signature_ok(
            "s3cret",
            "1790000000",
            b"{}",
            &sig,
            1_790_000_000
        ));
        assert!(!slack_signature_ok(
            "s3cret",
            "1790000000",
            body,
            &sig[3..],
            1_790_000_000
        ));
        assert!(!slack_signature_ok(
            "s3cret",
            "soon",
            body,
            &sig,
            1_790_000_000
        ));
    }

    #[test]
    fn whatsapp_signatures() {
        let body = br#"{"object":"whatsapp_business_account"}"#;
        let sig = format!("sha256={}", sign("app", &[body]));
        assert!(whatsapp_signature_ok("app", body, &sig));
        assert!(!whatsapp_signature_ok("app", b"{}", &sig));
        assert!(!whatsapp_signature_ok("app", body, "sha256=zz"));
        assert!(!whatsapp_signature_ok("app", body, &sig[7..]));
    }

    #[test]
    fn slack_events() {
        assert_eq!(
            parse_slack_event(br#"{"type":"url_verification","challenge":"abc"}"#).unwrap(),
            SlackEvent::UrlVerification("abc".into())
        );
        let message = br#"{"type":"event_callback","team_id":"T1","event":
            {"type":"message","channel":"C1","user":"U1","text":"hello","ts":"1.2"}}"#;
        assert_eq!(
            parse_slack_event(message).unwrap(),
            SlackEvent::Message {
                team: "T1".into(),
                channel: "C1".into(),
                message: Inbound {
                    external_id: "1.2".into(),
                    peer: "U1".into(),
                    peer_name: None,
                    body: "hello".into()
                }
            }
        );
        let bot = br#"{"type":"event_callback","team_id":"T1","event":
            {"type":"message","channel":"C1","bot_id":"B1","user":"U1","text":"echo","ts":"1.3"}}"#;
        assert_eq!(parse_slack_event(bot).unwrap(), SlackEvent::Ignored);
        let edit = br#"{"type":"event_callback","team_id":"T1","event":
            {"type":"message","subtype":"message_changed","channel":"C1","ts":"1.4"}}"#;
        assert_eq!(parse_slack_event(edit).unwrap(), SlackEvent::Ignored);
        assert!(parse_slack_event(b"nope").is_err());
    }

    #[test]
    fn whatsapp_deliveries() {
        let body = br#"{"object":"whatsapp_business_account","entry":[{"id":"W","changes":[{"field":"messages",
          "value":{"messaging_product":"whatsapp","metadata":{"display_phone_number":"15550001111","phone_number_id":"106"},
          "contacts":[{"profile":{"name":"Kerry"},"wa_id":"33612345678"}],
          "messages":[{"from":"33612345678","id":"wamid.A","timestamp":"1","type":"text","text":{"body":"hi"}},
                      {"from":"33612345678","id":"wamid.B","timestamp":"2","type":"image","image":{"id":"x"}}]}}]}]}"#;
        let parsed = parse_whatsapp(body).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "106");
        assert_eq!(parsed[0].1.body, "hi");
        assert_eq!(parsed[0].1.peer_name.as_deref(), Some("Kerry"));
        assert_eq!(parsed[1].1.body, "[image]");
        // Status updates carry no messages.
        let statuses = br#"{"entry":[{"changes":[{"value":{"metadata":{"phone_number_id":"106"},
          "statuses":[{"id":"wamid.A","status":"read"}]}}]}]}"#;
        assert!(parse_whatsapp(statuses).unwrap().is_empty());
    }

    #[test]
    fn signal_envelopes() {
        let body = br#"[{"envelope":{"source":"+33611111111","sourceNumber":"+33611111111","sourceName":"Ann",
            "timestamp":1790000000123,"dataMessage":{"timestamp":1790000000123,"message":"yo"}},"account":"+33600000000"},
          {"envelope":{"sourceNumber":"+33611111111","timestamp":1790000000999,"typingMessage":{"action":"STARTED"}}}]"#;
        assert_eq!(
            parse_signal(body).unwrap(),
            vec![Inbound {
                external_id: "+33611111111:1790000000123".into(),
                peer: "+33611111111".into(),
                peer_name: Some("Ann".into()),
                body: "yo".into()
            }]
        );
        assert!(parse_signal(b"{}").is_err());
    }

    #[test]
    fn sends_are_writes_under_base_url() {
        let connection = |provider: Provider, base: &str| Connection {
            id: "c".into(),
            provider,
            label: "l".into(),
            base_url: base.into(),
            external_id: None,
            status: "active".into(),
            owner_email: "e".into(),
            created_at: String::new(),
            last_checked_at: None,
            last_error: None,
            can_remove: true,
        };
        let routed = |external_id: &str, external_channel: Option<&str>| Routed {
            channel: Channel {
                id: "ch".into(),
                provider: Provider::Slack,
                label: "l".into(),
                connection_id: "c".into(),
                created_at: String::new(),
            },
            external_id: external_id.into(),
            external_channel: external_channel.map(str::to_string),
        };
        let (call, peer) = send_call(
            &routed("T1", Some("C1")),
            &connection(Provider::Slack, "https://slack.com/api"),
            "",
            "hi",
        )
        .unwrap();
        assert_eq!(call.url, "https://slack.com/api/chat.postMessage");
        assert_eq!(call.action, "write");
        assert_eq!(peer, "C1");
        let (call, peer) = send_call(
            &routed("106", None),
            &connection(Provider::Whatsapp, "https://graph.facebook.com/v21.0"),
            "+33 6 12 34 56 78",
            "hi",
        )
        .unwrap();
        assert_eq!(call.url, "https://graph.facebook.com/v21.0/106/messages");
        assert_eq!(peer, "33612345678");
        let (call, peer) = send_call(
            &routed("+33600000000", None),
            &connection(Provider::Signal, "https://signal.example.com"),
            "33611111111",
            "hi",
        )
        .unwrap();
        assert_eq!(call.url, "https://signal.example.com/v2/send");
        assert_eq!(peer, "+33611111111");
        assert!(call.body.unwrap().contains("\"number\":\"+33600000000\""));
        assert!(send_call(
            &routed("106", None),
            &connection(Provider::Whatsapp, "https://graph.facebook.com/v21.0"),
            "nobody",
            "hi"
        )
        .is_err());
    }

    #[test]
    fn sent_ids() {
        assert_eq!(
            sent_id(Provider::Slack, br#"{"ok":true,"ts":"1.5"}"#).unwrap(),
            Some("1.5".into())
        );
        assert!(sent_id(Provider::Slack, br#"{"ok":false,"error":"not_in_channel"}"#).is_err());
        assert_eq!(
            sent_id(Provider::Whatsapp, br#"{"messages":[{"id":"wamid.X"}]}"#).unwrap(),
            Some("wamid.X".into())
        );
        assert_eq!(
            sent_id(Provider::Signal, br#"{"timestamp":"1790"}"#).unwrap(),
            Some("1790".into())
        );
    }

    #[test]
    fn slack_channel_listing() {
        let body = br#"{"ok":true,"channels":[{"id":"C2","name":"general","is_private":false},
                                              {"id":"C1","name":"alerts","is_private":true}]}"#;
        let channels = parse_slack_channels(body).unwrap();
        assert_eq!(channels[0].name, "alerts");
        assert!(channels[0].private);
        assert!(parse_slack_channels(br#"{"ok":false,"error":"missing_scope"}"#).is_err());
    }
}
