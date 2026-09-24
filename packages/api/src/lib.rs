//! Shared fullstack server functions — the `core` service of
//! `docs/architecture.md` §2: users (signed in with GitHub or Google), their
//! orgs, and the third-party accounts connected to those orgs.
//!
//! The contract with the other services — `secrets`, `liaison`, `ledger` and
//! `typednotes-infra` — is `docs/connections.md`. Two rules from it shape
//! this crate:
//!
//! - **No credential ever reaches the client.** Tokens and keys go from the
//!   OAuth callback or a form straight into the vault; nothing here returns
//!   one, and the app's vault identity cannot even read them back.
//! - **Every org read is scoped to the caller's memberships.**
//!
//! The schema is applied by `typednotes-infra`, never by this server, so the
//! server's database identity has data rights only.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
mod server;

/// The routes the OAuth round trips need, to merge into the Dioxus router.
#[cfg(feature = "server")]
pub fn auth_routes() -> dioxus::server::axum::Router {
    server::routes::router()
}

#[cfg(feature = "server")]
use server::{connections, db, errors, session};

/// A signed-in user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
}

/// An org, as a member sees it. Ids and timestamps travel as text: the client
/// only displays them, and this keeps `uuid`/`chrono` out of the wasm build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Org {
    pub id: String,
    pub slug: String,
    pub name: String,
    /// The caller's role: `owner`, `admin` or `member`.
    pub role: String,
    pub created_at: String,
}

/// An org's page: the org and its spendable credits (`None` when `ledger`'s
/// tables are not there).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrgDetail {
    pub org: Org,
    pub credits: Option<i64>,
}

/// What this deployment can do, for the status line and for probing a fresh
/// deploy: each flag is a dependency that is reachable or configured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub database: bool,
    /// The app's own history is applied (through `connections`).
    pub schema: bool,
    /// `ledger`'s tables exist.
    pub ledger: bool,
    pub vault: bool,
    pub liaison: bool,
    pub github: bool,
    pub google: bool,
}

/// A kind of third-party account (docs/connections.md §3.1). The ids are
/// shared with the vault path, liaison's warrants and the `connections`
/// check constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Github,
    Gdrive,
    S3,
    Mistral,
    Openai,
    Anthropic,
    OpenaiCompatible,
}

impl Provider {
    pub const ALL: [Provider; 7] = [
        Provider::Github,
        Provider::Gdrive,
        Provider::S3,
        Provider::Mistral,
        Provider::Openai,
        Provider::Anthropic,
        Provider::OpenaiCompatible,
    ];

    /// The AI providers, connected with an API token.
    pub const AI: [Provider; 4] = [
        Provider::Mistral,
        Provider::Openai,
        Provider::Anthropic,
        Provider::OpenaiCompatible,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Provider::Github => "github",
            Provider::Gdrive => "gdrive",
            Provider::S3 => "s3",
            Provider::Mistral => "mistral",
            Provider::Openai => "openai",
            Provider::Anthropic => "anthropic",
            Provider::OpenaiCompatible => "openai-compatible",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Github => "GitHub",
            Provider::Gdrive => "Google Drive",
            Provider::S3 => "S3 bucket",
            Provider::Mistral => "Mistral",
            Provider::Openai => "OpenAI",
            Provider::Anthropic => "Anthropic",
            Provider::OpenaiCompatible => "OpenAI-compatible",
        }
    }

    /// The API root liaison confines calls to, when it does not depend on
    /// the account (S3 and OpenAI-compatible endpoints are entered by the user).
    pub fn fixed_base_url(self) -> Option<&'static str> {
        match self {
            Provider::Github => Some("https://api.github.com"),
            Provider::Gdrive => Some("https://www.googleapis.com"),
            Provider::Mistral => Some("https://api.mistral.ai/v1"),
            Provider::Openai => Some("https://api.openai.com/v1"),
            Provider::Anthropic => Some("https://api.anthropic.com/v1"),
            Provider::S3 | Provider::OpenaiCompatible => None,
        }
    }

    pub fn is_ai(self) -> bool {
        Self::AI.contains(&self)
    }
}

/// A connected account. Never carries its credential.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub provider: Provider,
    pub label: String,
    pub base_url: String,
    /// `pending`, `active` or `failed`.
    pub status: String,
    pub owner_email: String,
    pub created_at: String,
    pub last_checked_at: Option<String>,
    pub last_error: Option<String>,
    /// Whether the caller may remove it (its creator, or an org admin).
    pub can_remove: bool,
}

/// The outcome of a connection test through liaison.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
}

// ── Validation, identical on both sides ─────────────────────────────────
// The forms explain before a round trip; the server still enforces.

/// Why an org name or slug was refused.
pub fn validate_org(slug: &str, name: &str) -> Result<(), String> {
    let slug_ok = (3..=40).contains(&slug.len())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-');
    if !slug_ok {
        return Err(
            "slug: 3–40 characters, lowercase letters, digits and inner dashes".to_string(),
        );
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err("name: 1–100 characters".to_string());
    }
    Ok(())
}

/// An API base URL: `https://host[:port][/path]`, no query, fragment,
/// userinfo or dot segments, returned without a trailing `/`. Plain `http`
/// only for `localhost`/`127.0.0.1` (local S3 or model servers). liaison
/// re-checks all of this before any call.
pub fn validate_base_url(url: &str, allow_path: bool) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let rest = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split(['/', ':']).next().unwrap_or("");
        if host != "localhost" && host != "127.0.0.1" {
            return Err("the URL must start with https://".to_string());
        }
        rest
    } else {
        return Err("the URL must start with https://".to_string());
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let bad = |c: char| c.is_whitespace() || c.is_control() || "?#@\\\"<>{}|^`".contains(c);
    if authority.is_empty() || url.chars().any(bad) {
        return Err(
            "the URL must be a plain https://host[/path], without query or credentials".to_string(),
        );
    }
    if path
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
        && !path.is_empty()
    {
        return Err("the URL path is malformed".to_string());
    }
    if !allow_path && !path.is_empty() {
        return Err(
            "the endpoint must not have a path, e.g. https://s3.fr-par.scw.cloud".to_string(),
        );
    }
    Ok(url.to_string())
}

fn is_token(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.chars().all(|c| c.is_ascii_graphic())
}

/// An API token or key: printable ASCII, no spaces.
pub fn validate_api_key(key: &str) -> Result<String, String> {
    let key = key.trim();
    if is_token(key, 8, 512) {
        Ok(key.to_string())
    } else {
        Err("the API key must be 8–512 printable characters without spaces".to_string())
    }
}

/// A normalized S3 connection form.
#[derive(Clone, Debug, PartialEq)]
pub struct S3Form {
    /// `{endpoint}/{bucket}`: path-style, so liaison confines calls to the bucket.
    pub base_url: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

pub fn validate_s3(
    endpoint: &str,
    region: &str,
    bucket: &str,
    access_key_id: &str,
    secret_access_key: &str,
) -> Result<S3Form, String> {
    let endpoint = validate_base_url(endpoint, false)?;
    let region = region.trim().to_string();
    if !(1..=32).contains(&region.len())
        || !region
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("region: lowercase letters, digits and dashes, e.g. fr-par".to_string());
    }
    let bucket = bucket.trim().to_string();
    let alnum = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if !(3..=63).contains(&bucket.len())
        || !bucket
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        || !alnum(bucket.chars().next())
        || !alnum(bucket.chars().last())
    {
        return Err("bucket: 3–63 lowercase letters, digits, dots and dashes".to_string());
    }
    let (access_key_id, secret_access_key) = (access_key_id.trim(), secret_access_key.trim());
    if !is_token(access_key_id, 1, 256) || !is_token(secret_access_key, 1, 256) {
        return Err("access key id and secret: printable characters without spaces".to_string());
    }
    Ok(S3Form {
        base_url: format!("{endpoint}/{bucket}"),
        region,
        bucket,
        access_key_id: access_key_id.to_string(),
        secret_access_key: secret_access_key.to_string(),
    })
}

// ── Server functions ────────────────────────────────────────────────────

/// Reachability and configuration, for the status line.
#[get("/api/health")]
pub async fn health() -> Result<Health, ServerFnError> {
    Ok(db::health().await)
}

/// The signed-in user, if any.
#[get("/api/me")]
pub async fn current_user() -> Result<Option<User>, ServerFnError> {
    session::current_user().await
}

/// End the session and clear its cookie.
#[post("/api/logout")]
pub async fn logout() -> Result<(), ServerFnError> {
    use dioxus::fullstack::{FullstackContext, HeaderValue};
    let headers = session::request_headers().await?;
    session::destroy(&headers).await?;
    let secure = server::config::public_url(&headers).starts_with("https://");
    if let Some(ctx) = FullstackContext::current() {
        if let Ok(value) = HeaderValue::from_str(&session::clear_cookie(secure)) {
            ctx.add_response_header(dioxus::fullstack::http::header::SET_COOKIE, value);
        }
    }
    Ok(())
}

/// The caller's orgs, newest first.
#[get("/api/orgs")]
pub async fn list_orgs() -> Result<Vec<Org>, ServerFnError> {
    let user = session::require_user().await?;
    db::list_orgs_for(&user.id).await
}

/// Create an org owned by the caller, with `ledger`'s welcome grant. `409` if
/// the slug is taken, `400` if it is malformed.
#[post("/api/orgs")]
pub async fn create_org(slug: String, name: String) -> Result<Org, ServerFnError> {
    let user = session::require_user().await?;
    let slug = slug.trim().to_lowercase();
    validate_org(&slug, &name).map_err(errors::bad_request)?;
    db::create_org_for(&user.id, &slug, name.trim()).await
}

/// The caller's org and the user; `404` for non-members.
#[cfg(feature = "server")]
async fn member_org(slug: &str) -> Result<(User, Org), ServerFnError> {
    let user = session::require_user().await?;
    let org = db::org_for_member(slug.trim(), &user.id)
        .await?
        .ok_or_else(|| errors::not_found("no such organisation"))?;
    Ok((user, org))
}

/// An org's page.
#[post("/api/org")]
pub async fn get_org(slug: String) -> Result<OrgDetail, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    let credits = db::available_credits(&org.id).await;
    Ok(OrgDetail { org, credits })
}

/// The org's connections, newest first.
#[post("/api/connections")]
pub async fn list_connections(slug: String) -> Result<Vec<Connection>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    connections::list(&org, &user).await
}

/// Connect an S3-compatible bucket with an access key.
#[post("/api/connections/s3")]
pub async fn connect_s3(
    slug: String,
    endpoint: String,
    region: String,
    bucket: String,
    access_key_id: String,
    secret_access_key: String,
) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    let form = validate_s3(
        &endpoint,
        &region,
        &bucket,
        &access_key_id,
        &secret_access_key,
    )
    .map_err(errors::bad_request)?;
    let credential = server::vault::s3(
        &form.base_url,
        &form.region,
        &form.access_key_id,
        &form.secret_access_key,
    );
    connections::store(
        &org,
        &user,
        Provider::S3,
        &form.bucket,
        &form.base_url,
        credential,
    )
    .await
}

/// Connect an AI account with an API token. `base_url` is used only for
/// `OpenaiCompatible`.
#[post("/api/connections/ai")]
pub async fn connect_ai(
    slug: String,
    provider: Provider,
    api_key: String,
    base_url: String,
) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    if !provider.is_ai() {
        return Err(errors::bad_request("not an AI provider"));
    }
    let key = validate_api_key(&api_key).map_err(errors::bad_request)?;
    let base_url = match provider.fixed_base_url() {
        Some(fixed) => fixed.to_string(),
        None => validate_base_url(&base_url, true).map_err(errors::bad_request)?,
    };
    let (label, credential) = match provider {
        Provider::Anthropic => (
            provider.name().to_string(),
            server::vault::header(
                &base_url,
                "x-api-key",
                &key,
                &[("anthropic-version", "2023-06-01")],
            ),
        ),
        Provider::OpenaiCompatible => {
            let host = base_url.split("://").nth(1).unwrap_or(&base_url);
            (host.to_string(), server::vault::bearer(&base_url, &key))
        }
        _ => (
            provider.name().to_string(),
            server::vault::bearer(&base_url, &key),
        ),
    };
    connections::store(&org, &user, provider, &label, &base_url, credential).await
}

/// Remove a connection and its credential.
#[post("/api/connections/delete")]
pub async fn delete_connection(slug: String, id: String) -> Result<(), ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    connections::remove(&org, &user, id.trim()).await
}

/// Test a connection end to end through liaison.
#[post("/api/connections/test")]
pub async fn test_connection(slug: String, id: String) -> Result<TestResult, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    connections::test(&org, &user, id.trim()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_plain_slug() {
        assert!(validate_org("acme-labs", "Acme Labs").is_ok());
    }

    #[test]
    fn refuses_malformed_slugs() {
        for slug in ["ab", "Acme", "acme_labs", "-acme", "acme-", "acme labs"] {
            assert!(
                validate_org(slug, "Acme").is_err(),
                "{slug} should be refused"
            );
        }
    }

    #[test]
    fn refuses_blank_or_long_names() {
        assert!(validate_org("acme", "   ").is_err());
        assert!(validate_org("acme", &"x".repeat(101)).is_err());
    }

    #[test]
    fn provider_ids_round_trip() {
        for p in Provider::ALL {
            assert_eq!(Provider::from_id(p.id()), Some(p));
        }
        assert_eq!(Provider::from_id("dropbox"), None);
        assert!(Provider::Anthropic.is_ai() && !Provider::S3.is_ai());
    }

    #[test]
    fn base_urls() {
        assert_eq!(
            validate_base_url("https://api.example.com/v1/", true).unwrap(),
            "https://api.example.com/v1"
        );
        assert_eq!(
            validate_base_url("http://localhost:9000", false).unwrap(),
            "http://localhost:9000"
        );
        for bad in [
            "http://api.example.com",
            "ftp://x",
            "https://",
            "https://user@host",
            "https://host/v1?x=1",
            "https://host/#f",
            "https://host/a/../b",
            "https://host//v1",
            "https://ho st",
        ] {
            assert!(
                validate_base_url(bad, true).is_err(),
                "{bad} should be refused"
            );
        }
        assert!(validate_base_url("https://s3.example.com/path", false).is_err());
    }

    #[test]
    fn s3_forms() {
        let f = validate_s3(
            "https://s3.fr-par.scw.cloud/",
            "fr-par",
            "my-bucket",
            "AK",
            "SK",
        )
        .unwrap();
        assert_eq!(f.base_url, "https://s3.fr-par.scw.cloud/my-bucket");
        assert!(validate_s3("https://s3.x", "FR", "b-1", "a", "s").is_err());
        assert!(validate_s3("https://s3.x", "fr", "-b", "a", "s").is_err());
        assert!(validate_s3("https://s3.x", "fr", "bkt", "a b", "s").is_err());
    }

    #[test]
    fn api_keys() {
        assert_eq!(validate_api_key("  sk-12345678 ").unwrap(), "sk-12345678");
        assert!(validate_api_key("short").is_err());
        assert!(validate_api_key("has a space in it").is_err());
    }
}
