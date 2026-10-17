//! Trusted hosted linked-device provisioning. The app creates a random vault
//! key but never reads device credentials; Liaison owns all protocol IO.
use dioxus::prelude::ServerFnError;
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use crate::{Connection, DevicePairing, Org, Provider, User, PAIRED_DEVICE_BASE};
use super::{connector, connections, errors, liaison, warrant};

pub fn supported(provider: Provider) -> bool { matches!(provider, Provider::Signal | Provider::Whatsapp) }

/// Callers already hold the org mint/deletion lock. These warrants never leave
/// the trusted app. Binding the owner in the signed action prevents rebinding.
pub async fn control(org: &str, owner: &str, connection: &str, provider: Provider, action: &str) -> Result<Vec<u8>, ServerFnError> {
    if !supported(provider) || !["pair", "status", "unlink"].contains(&action) { return Err(errors::bad_request("unsupported device operation")); }
    let root = warrant::root_key().map_err(errors::unavailable)?;
    let action = format!("device.{action}:{owner}");
    let account = format!("{owner}/{connection}");
    let now = warrant::now(); let run = uuid::Uuid::new_v4().to_string();
    let token = warrant::mint(&root, &uuid::Uuid::new_v4().to_string(), org, vec![
        warrant::Caveat::ExpiresAt(now + 60),
        warrant::Caveat::Capability { provider: provider.id().into(), action: action.clone() },
        warrant::Caveat::Resource(connection.into()), warrant::Caveat::Budget(0), warrant::Caveat::RunId(run.clone()),
    ]);
    let request = liaison::Request { now, cost: 0, provider: provider.id(), action: &action, resource: connection, run_id: &run, org_id: org };
    let call = liaison::Call { account, operation: action.clone(), resource: vec![], payload: "{}".into() };
    match liaison::device(&token, &request, &call).await.map_err(errors::bad_gateway)? {
        liaison::Outcome::Upstream { status, body } if status == 200 => Ok(body),
        liaison::Outcome::Upstream { .. } => Err(errors::bad_gateway("The linked device could not be reached. Retry or unlink it from your phone")),
        liaison::Outcome::Refused { error, .. } => Err(errors::bad_gateway(format!("Linked-device broker refused: {error}"))),
    }
}

pub async fn start(org: &Org, user: &User, provider: Provider) -> Result<DevicePairing, ServerFnError> {
    if !supported(provider) { return Err(errors::bad_request("choose Signal or WhatsApp")); }
    if !matches!(org.role.as_str(), "owner" | "admin") { return Err(errors::forbidden("only organization owners and admins can link a messaging account")); }
    let broker = super::config::env("LIAISON_URL").ok_or_else(|| errors::unavailable("the connector broker is not configured"))?;
    let available: serde_json::Value = super::oauth::http().get(format!("{}/_health/paired", broker.trim_end_matches('/')))
        .send().await.map_err(|_| errors::unavailable("the linked-device broker is unreachable"))?
        .json().await.map_err(|_| errors::unavailable("the broker does not support native QR pairing"))?;
    if available.get(provider.id()).and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(errors::unavailable(format!("{} QR pairing is not enabled on the connector broker", provider.name())));
    }
    let mut key = [0; 32]; getrandom::fill(&mut key).map_err(|_| errors::unavailable("random source unavailable"))?;
    let connection = connections::store(org, user, connections::NewConnection {
        provider, label: &format!("{} linked device", provider.name()), base_url: PAIRED_DEVICE_BASE, external_id: None,
    }, json!({"kind":"paired_device", "base_url": PAIRED_DEVICE_BASE, "key":hex::encode(key)})).await?;
    status(org, user, &connection.id, true).await
}

pub async fn status(org: &Org, user: &User, id: &str, pair: bool) -> Result<DevicePairing, ServerFnError> {
    if let Some(context) = dioxus::fullstack::FullstackContext::current() {
        context.add_response_header(dioxus::fullstack::http::header::CACHE_CONTROL,
            dioxus::fullstack::http::HeaderValue::from_static("no-store"));
    }
    let mut tx = connector::lock(&org.id).await?;
    connector::require_actor(&mut tx, &org.id, &user.id).await?;
    let row = sqlx::query("select user_id::text as owner, provider, base_url from connections where org_id=$1::uuid and id=$2::uuid")
        .bind(&org.id).bind(id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?
        .ok_or_else(|| errors::not_found("no such connection"))?;
    let owner: String = row.get("owner");
    let role = sqlx::query("select role from memberships where org_id=$1::uuid and user_id=$2::uuid")
        .bind(&org.id).bind(&user.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?.get::<String,_>("role");
    if owner != user.id && !matches!(role.as_str(), "owner" | "admin") { return Err(errors::forbidden("only the connection owner or an organization admin can manage this device")); }
    if row.get::<String,_>("base_url") != PAIRED_DEVICE_BASE { return Err(errors::bad_request("this connection does not use QR pairing")); }
    let provider = Provider::from_id(&row.get::<String,_>("provider")).ok_or_else(|| errors::bad_request("unknown provider"))?;
    let body = control(&org.id, &owner, id, provider, if pair { "pair" } else { "status" }).await?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct State { state: String, identity: Option<String>, qr: Option<String>, expires_at: u64, error: Option<String> }
    let state: State = serde_json::from_slice(&body).map_err(|_| errors::bad_gateway("invalid linked-device state"))?;
    if !["connecting", "pairing", "connected", "disconnected", "expired"].contains(&state.state.as_str()) { return Err(errors::bad_gateway("unknown linked-device state")); }
    if state.state == "connected" {
        let identity = state.identity.as_deref().ok_or_else(|| errors::bad_gateway("paired device did not identify its account"))?;
        crate::ConnectorPermissions::valid_resource(&[identity.into()]).then_some(()).ok_or_else(|| errors::bad_gateway("invalid paired identity"))?;
        let updated = sqlx::query("update connections set external_id=$2, label=$3, last_checked_at=now(), last_error=null where id=$1::uuid and (external_id is null or external_id=$2)")
            .bind(id).bind(identity).bind(format!("{} {identity}", provider.name())).execute(&mut *tx).await.map_err(errors::db_error)?;
        if updated.rows_affected() != 1 { return Err(errors::bad_gateway("the paired account identity changed; unlink before reconnecting")); }
    }
    let qr_svg = state.qr.filter(|qr| qr.len() <= 4096 && state.expires_at > warrant::now()).map(|qr| {
        let code = qrcode::QrCode::new(qr.as_bytes()).map_err(|_| errors::bad_gateway("could not render the pairing code"))?;
        Ok::<_,ServerFnError>(code.render::<qrcode::render::svg::Color>().min_dimensions(256,256).build())
    }).transpose()?;
    tx.commit().await.map_err(errors::db_error)?;
    Ok(DevicePairing { connection: id.into(), state: state.state, identity: state.identity, qr_svg, expires_at: state.expires_at, error: state.error })
}

pub async fn unlink_locked(org: &str, owner: &str, id: &str, provider: Provider, base: &str) -> Result<(), ServerFnError> {
    if supported(provider) && base == PAIRED_DEVICE_BASE { control(org, owner, id, provider, "unlink").await?; }
    Ok(())
}

pub async fn unlink(org: &Org, user: &User, id: &str) -> Result<(), ServerFnError> {
    // Connection removal closes all live grants before unlinking and erasing keys.
    connections::remove(org, user, id).await
}

/// Only trusted non-secret connection metadata determines the native transport.
pub fn is_paired(connection: &Connection) -> bool { supported(connection.provider) && connection.base_url == PAIRED_DEVICE_BASE }
