//! JSON over HTTP to the internal services the app drives — `lode` and
//! `lun` — with their bearer token. Errors are `{"error": "…"}` on both.

use std::time::Duration;

use serde_json::Value;

use super::config::Service;
use super::oauth::http;

/// A service's answer: its status and JSON body (`null` if it had none).
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

impl Answer {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The service's `error`, or a snippet of what it said.
    pub fn error(&self) -> String {
        match self.body.get("error").and_then(Value::as_str) {
            Some(e) => e.to_string(),
            None => self.body.to_string().chars().take(200).collect(),
        }
    }
}

pub async fn call(
    svc: &Service,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Answer, String> {
    let mut request = http()
        .request(method, format!("{}{path}", svc.url))
        .bearer_auth(&svc.token)
        .timeout(timeout);
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|e| {
        if e.is_timeout() {
            "timed out".to_string()
        } else {
            format!("unreachable: {e}")
        }
    })?;
    let status = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|e| format!("unreadable answer: {e}"))?;
    let body = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    };
    Ok(Answer { status, body })
}

/// `call`, with the default timeout of a quick request.
pub async fn quick(
    svc: &Service,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Answer, String> {
    call(svc, method, path, body, Duration::from_secs(30)).await
}

/// A path segment, percent-encoded (ids and names the services gave us are
/// hex or identifiers, but nothing downstream should have to trust that).
pub fn segment(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}
