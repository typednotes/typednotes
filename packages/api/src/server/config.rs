//! Environment configuration (docs/connections.md §8). Every variable is read
//! when used, not at startup: a missing one disables its feature and says so,
//! rather than stopping a server that can still serve everything else.

use dioxus::fullstack::HeaderMap;

/// A non-empty environment variable.
pub fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// An OAuth client registered with a provider.
pub struct OAuthClient {
    pub id: String,
    pub secret: String,
}

fn client(id: &str, secret: &str) -> Option<OAuthClient> {
    Some(OAuthClient {
        id: env(id)?,
        secret: env(secret)?,
    })
}

/// The GitHub OAuth App: sign-in and the `github` connection.
pub fn github() -> Option<OAuthClient> {
    client("GITHUB_CLIENT_ID", "GITHUB_CLIENT_SECRET")
}

/// The Google OAuth client: sign-in, Drive, Calendar and Gmail connections.
pub fn google() -> Option<OAuthClient> {
    client("GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET")
}

/// Microsoft Graph: personal Outlook and work/school Microsoft 365 accounts.
/// The app and liaison use the same client pair and the fixed `common` tenant.
pub fn microsoft() -> Option<OAuthClient> {
    client("MICROSOFT_CLIENT_ID", "MICROSOFT_CLIENT_SECRET")
}

/// The GitLab (gitlab.com) OAuth application: the `gitlab` connection.
/// liaison needs the same pair to refresh its tokens.
pub fn gitlab() -> Option<OAuthClient> {
    client("GITLAB_CLIENT_ID", "GITLAB_CLIENT_SECRET")
}

/// The Dropbox app: the `dropbox` connection. liaison needs the same pair to
/// refresh its tokens.
pub fn dropbox() -> Option<OAuthClient> {
    client("DROPBOX_CLIENT_ID", "DROPBOX_CLIENT_SECRET")
}

/// The Slack app: installing it in a workspace is the `slack` connection.
pub fn slack() -> Option<OAuthClient> {
    client("SLACK_CLIENT_ID", "SLACK_CLIENT_SECRET")
}

/// The Slack app's signing secret, which authenticates its Events API
/// requests to `/hooks/slack`.
pub fn slack_signing_secret() -> Option<String> {
    env("SLACK_SIGNING_SECRET")
}

/// The Meta app's secret, which signs WhatsApp webhook deliveries, and the
/// verify token Meta echoes when the webhook is registered.
pub fn whatsapp_webhook() -> Option<(String, String)> {
    Some((env("WHATSAPP_APP_SECRET")?, env("WHATSAPP_VERIFY_TOKEN")?))
}

/// An internal service the app calls with a bearer token.
#[derive(Clone)]
pub struct Service {
    pub url: String,
    pub token: String,
}

fn service(url: &str, token: &str) -> Option<Service> {
    Some(Service {
        url: env(url)?.trim_end_matches('/').to_string(),
        token: env(token)?,
    })
}

/// `lode`, the implementer (docs/services/lode.md): `LODE_URL`, `LODE_TOKEN`.
pub fn lode() -> Option<Service> {
    service("LODE_URL", "LODE_TOKEN")
}

/// `lun`, the runtime (docs/services/lun.md): `LUN_URL`, `LUN_TOKEN`.
pub fn lun() -> Option<Service> {
    service("LUN_URL", "LUN_TOKEN")
}

/// `compute-db`, where `db` sinks write (docs/computations.md §4.1): a
/// connection string whose identity may create roles and schemas there, and
/// nowhere else.
pub fn compute_db_url() -> Option<String> {
    env("COMPUTE_DB_URL")
}

/// Address visible to the remote runner, when the app uses a local tunnel.
pub fn runtime_liaison_url() -> Option<String> {
    env("LIAISON_RUNTIME_URL").or_else(|| env("LIAISON_URL"))
}

pub fn background_enabled() -> bool {
    !env("TYPEDNOTES_BACKGROUND").is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "off"))
}

/// A non-negative integer setting, with a default.
pub fn number(name: &str, default: i64) -> i64 {
    match env(name) {
        None => default,
        Some(v) => v
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 0)
            .unwrap_or_else(|| {
                eprintln!("{name}={v:?} is not a non-negative integer; using {default}");
                default
            }),
    }
}

/// Scheduled and watch checks an org may run per hour (§3.1's rate cap).
pub fn source_checks_per_hour() -> i64 {
    number("TYPEDNOTES_SOURCE_CHECKS_PER_HOUR", 120)
}

/// Deliveries one endpoint accepts per minute (§3.5).
pub fn endpoint_calls_per_minute() -> i64 {
    number("TYPEDNOTES_ENDPOINT_CALLS_PER_MINUTE", 60)
}

/// The credits liaison holds for each of lode's model calls (§5).
pub fn model_call_cost() -> u64 {
    number("TYPEDNOTES_MODEL_CALL_COST", 10) as u64
}

/// Rewrites the app may launch by itself — for a failed build, or an error
/// lun reports — before a member asks for one again (0 disables them): the
/// default of orgs that did not set their own on their settings page.
pub fn auto_repairs() -> i64 {
    number("TYPEDNOTES_AUTO_REPAIRS", 2)
}

/// The per-call budget of a `storage` sink's write warrant (§4.3).
pub fn storage_write_cost() -> u64 {
    number("TYPEDNOTES_STORAGE_WRITE_COST", 1) as u64
}

/// Credits granted to a new org (`ledger`'s welcome grant). `0` disables it.
pub fn welcome_credits() -> i64 {
    number("TYPEDNOTES_WELCOME_CREDITS", 1000)
}

/// The app's public origin, for OAuth redirect URIs: `PUBLIC_URL` if set,
/// else rebuilt from the request. A forged `Host` only yields a redirect URI
/// the provider refuses, since each provider checks it against the ones
/// registered for the client.
pub fn public_url(headers: &HeaderMap) -> String {
    if let Some(url) = env("PUBLIC_URL") {
        return url.trim_end_matches('/').to_string();
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let host = header("x-forwarded-host")
        .or_else(|| header("host"))
        .unwrap_or_else(|| "localhost:8080".to_string());
    let local = host.starts_with("localhost") || host.starts_with("127.0.0.1");
    let proto = header("x-forwarded-proto")
        .unwrap_or_else(|| if local { "http" } else { "https" }.to_string());
    format!("{proto}://{host}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::fullstack::HeaderValue;

    #[test]
    fn public_url_prefers_forwarded_headers() {
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_static("internal:8080"));
        h.insert(
            "x-forwarded-host",
            HeaderValue::from_static("app.example.com"),
        );
        h.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        assert_eq!(public_url(&h), "https://app.example.com");
    }

    #[test]
    fn public_url_defaults_to_http_on_localhost_only() {
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_static("localhost:8080"));
        assert_eq!(public_url(&h), "http://localhost:8080");
        h.insert("host", HeaderValue::from_static("app.example.com"));
        assert_eq!(public_url(&h), "https://app.example.com");
    }
}
