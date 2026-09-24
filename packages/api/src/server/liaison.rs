//! The app's client for liaison's `POST /v0/egress` (docs/connections.md §5).
//!
//! The app is `core`: it mints the warrant and asks liaison to make the call.
//! It never sees the credential — liaison fetches it from the vault, attaches
//! it, and returns only the provider's response.

use serde::Deserialize;
use serde_json::{json, Value};

use super::config::env;
use super::oauth::http;
use super::warrant::Warrant;

pub fn configured() -> bool {
    env("LIAISON_URL").is_some() && env("LIAISON_ROOT_KEY").is_some()
}

/// One outbound call, as liaison's `call` object.
pub struct Call<'a> {
    pub account: String,
    pub method: &'a str,
    pub url: String,
    pub headers: Vec<(&'a str, &'a str)>,
}

/// The request fields next to `warrant` and `call`.
pub struct Request<'a> {
    pub now: u64,
    pub cost: u64,
    pub provider: &'a str,
    pub action: &'a str,
    pub resource: &'a str,
    pub run_id: &'a str,
    pub org_id: &'a str,
}

pub enum Outcome {
    /// The provider answered (any status).
    Upstream { status: u16, body: Vec<u8> },
    /// Liaison refused or failed: its HTTP status and `error` code.
    Refused { status: u16, error: String },
}

pub fn body(warrant: &Warrant, r: &Request, call: &Call) -> Value {
    let headers: serde_json::Map<String, Value> = call
        .headers
        .iter()
        .map(|(k, v)| (k.to_string(), Value::from(*v)))
        .collect();
    json!({
        "warrant": warrant.to_json(),
        "now": r.now.to_string(),
        "cost": r.cost.to_string(),
        "provider": r.provider,
        "action": r.action,
        "resource": r.resource,
        "runId": r.run_id,
        "orgId": r.org_id,
        "call": {
            "kind": "provider",
            "account": call.account,
            "method": call.method,
            "url": call.url,
            "headers": headers,
        },
    })
}

pub async fn egress(
    warrant: &Warrant,
    r: &Request<'_>,
    call: &Call<'_>,
) -> Result<Outcome, String> {
    let base = env("LIAISON_URL").ok_or("LIAISON_URL is not set")?;
    let response = http()
        .post(format!("{}/v0/egress", base.trim_end_matches('/')))
        .json(&body(warrant, r, call))
        .send()
        .await
        .map_err(|e| format!("liaison unreachable: {e}"))?;
    let status = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|e| format!("liaison answer unreadable: {e}"))?;
    if status == 200 {
        #[derive(Deserialize)]
        struct Upstream {
            status: u16,
            body: String,
        }
        let up: Upstream =
            serde_json::from_str(&text).map_err(|e| format!("liaison answer unreadable: {e}"))?;
        let body = hex::decode(&up.body).map_err(|_| "liaison body is not hex".to_string())?;
        return Ok(Outcome::Upstream {
            status: up.status,
            body,
        });
    }
    let error = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| text.chars().take(120).collect());
    Ok(Outcome::Refused { status, error })
}
