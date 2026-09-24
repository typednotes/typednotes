//! The app's client for `typednotes/secrets` (docs/connections.md §4).
//!
//! The app logs in as the `typednotes-app` userpass identity, whose policy
//! grants `create` and `delete` under `secret/data/thirdparty/` and nothing
//! else — in particular not `read`: the app puts credentials into the vault
//! and can never take them out again. Only liaison reads them.
//!
//! Vault tokens last an hour; the token is cached and replaced five minutes
//! before it expires, and once more if the vault answers `403` (revoked, or
//! the vault restarted with a new token store).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use super::config::env;
use super::oauth::http;
use crate::Provider;

struct Settings {
    url: String,
    username: String,
    password: String,
}

fn settings() -> Option<Settings> {
    Some(Settings {
        url: env("SECRETS_URL")?.trim_end_matches('/').to_string(),
        username: env("SECRETS_USERNAME").unwrap_or_else(|| "typednotes-app".to_string()),
        password: env("SECRETS_PASSWORD")?,
    })
}

pub fn configured() -> bool {
    settings().is_some()
}

static TOKEN: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// The vault path (relative to `secret/data/`) of a connection's credential.
pub fn credential_path(provider: Provider, user_id: &str, connection_id: &str) -> String {
    format!("thirdparty/{}/{user_id}/{connection_id}", provider.id())
}

async fn login(s: &Settings) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Auth {
        client_token: String,
        lease_duration: u64,
    }
    #[derive(Deserialize)]
    struct Login {
        auth: Auth,
    }
    let response = http()
        .post(format!("{}/v1/auth/userpass/login", s.url))
        .json(&json!({ "username": s.username, "password": s.password }))
        .send()
        .await
        .map_err(|e| format!("vault unreachable: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "vault login as {} refused: {}",
            s.username,
            response.status()
        ));
    }
    let login: Login = response
        .json()
        .await
        .map_err(|e| format!("vault login unreadable: {e}"))?;
    // Renew five minutes early; a zero lease means "does not expire".
    let lifetime = match login.auth.lease_duration {
        0 => Duration::from_secs(365 * 24 * 3600),
        secs => Duration::from_secs(secs.saturating_sub(300).max(30)),
    };
    let token = login.auth.client_token;
    *TOKEN.lock().expect("vault token lock") = Some((token.clone(), Instant::now() + lifetime));
    Ok(token)
}

async fn token(s: &Settings) -> Result<String, String> {
    let cached = TOKEN.lock().expect("vault token lock").clone();
    match cached {
        Some((token, renew_at)) if Instant::now() < renew_at => Ok(token),
        _ => login(s).await,
    }
}

/// Send one request with the cached token, logging in again and retrying
/// once on `403`.
async fn send(
    s: &Settings,
    build: impl Fn(&str) -> reqwest::RequestBuilder,
) -> Result<reqwest::Response, String> {
    let first = build(&token(s).await?)
        .send()
        .await
        .map_err(|e| format!("vault unreachable: {e}"))?;
    if first.status() != reqwest::StatusCode::FORBIDDEN {
        return Ok(first);
    }
    let fresh = login(s).await?;
    build(&fresh)
        .send()
        .await
        .map_err(|e| format!("vault unreachable: {e}"))
}

/// Write `credential` at `secret/data/{path}` (a new version if it exists).
pub async fn write(path: &str, credential: &Value) -> Result<(), String> {
    let s = settings().ok_or("the vault is not configured (SECRETS_URL, SECRETS_PASSWORD)")?;
    let url = format!("{}/v1/secret/data/{path}", s.url);
    let response = send(&s, |token| {
        http().post(&url).bearer_auth(token).json(credential)
    })
    .await?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("vault refused the write: {}", response.status()))
    }
}

/// Delete the credential at `secret/data/{path}`. Already absent is success.
pub async fn delete(path: &str) -> Result<(), String> {
    let s = settings().ok_or("the vault is not configured (SECRETS_URL, SECRETS_PASSWORD)")?;
    let url = format!("{}/v1/secret/data/{path}", s.url);
    let response = send(&s, |token| http().delete(&url).bearer_auth(token)).await?;
    match response.status() {
        s if s.is_success() || s == reqwest::StatusCode::NOT_FOUND => Ok(()),
        s => Err(format!("vault refused the delete: {s}")),
    }
}

// ── Credential documents (docs/connections.md §3.3) ─────────────────────
// Every value is a string, numbers included, so liaison never parses a float.

pub fn bearer(base_url: &str, token: &str) -> Value {
    json!({ "kind": "bearer", "base_url": base_url, "token": token })
}

pub fn header(base_url: &str, header: &str, token: &str, headers: &[(&str, &str)]) -> Value {
    let headers: serde_json::Map<String, Value> = headers
        .iter()
        .map(|(k, v)| (k.to_string(), Value::from(*v)))
        .collect();
    json!({ "kind": "header", "base_url": base_url, "header": header, "token": token,
            "headers": headers })
}

pub fn google_oauth(
    base_url: &str,
    access_token: &str,
    refresh_token: &str,
    expires_at: u64,
) -> Value {
    json!({ "kind": "google_oauth", "base_url": base_url, "access_token": access_token,
            "refresh_token": refresh_token, "expires_at": expires_at.to_string() })
}

pub fn s3(base_url: &str, region: &str, access_key_id: &str, secret_access_key: &str) -> Value {
    json!({ "kind": "s3", "base_url": base_url, "region": region,
            "access_key_id": access_key_id, "secret_access_key": secret_access_key })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_provider_user_connection() {
        assert_eq!(
            credential_path(Provider::Gdrive, "u1", "c1"),
            "thirdparty/gdrive/u1/c1"
        );
        assert_eq!(
            credential_path(Provider::OpenaiCompatible, "u", "c"),
            "thirdparty/openai-compatible/u/c"
        );
    }

    /// The shapes liaison's `Credential.parse` accepts.
    #[test]
    fn credential_shapes() {
        assert_eq!(
            bearer("https://api.github.com", "gho_x"),
            json!({"kind": "bearer", "base_url": "https://api.github.com", "token": "gho_x"})
        );
        assert_eq!(
            header(
                "https://api.anthropic.com/v1",
                "x-api-key",
                "k",
                &[("anthropic-version", "2023-06-01")]
            ),
            json!({"kind": "header", "base_url": "https://api.anthropic.com/v1", "header": "x-api-key",
                   "token": "k", "headers": {"anthropic-version": "2023-06-01"}})
        );
        assert_eq!(
            google_oauth("https://www.googleapis.com", "a", "r", 1_790_000_000)["expires_at"],
            json!("1790000000")
        );
        assert_eq!(
            s3("https://s3.fr-par.scw.cloud/b", "fr-par", "AK", "SK"),
            json!({"kind": "s3", "base_url": "https://s3.fr-par.scw.cloud/b", "region": "fr-par",
                   "access_key_id": "AK", "secret_access_key": "SK"})
        );
    }
}
