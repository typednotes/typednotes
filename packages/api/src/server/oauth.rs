//! OAuth 2.0 authorization-code flows with PKCE, against GitHub and Google,
//! for both purposes the app has (docs/connections.md §2–3): signing in, and
//! connecting a `github` or `gdrive` account.
//!
//! Each round trip is a row in `oauth_flows`, keyed by the `state` sent to the
//! provider and holding the PKCE verifier. The callback *deletes* the row it
//! resumes, so a `state` is single-use and a replayed callback finds nothing.

use std::sync::OnceLock;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use dioxus::prelude::ServerFnError;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::config::OAuthClient;
use super::db::pool;
use super::errors::db_error;
use super::session::{random_token, ProviderIdentity};
use crate::Provider;

/// A shared HTTP client for provider calls: bounded, and identifying itself
/// (GitHub's API refuses requests without a `User-Agent`).
pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("typednotes")
            .timeout(Duration::from_secs(20))
            .build()
            .expect("a default reqwest client builds")
    })
}

/// The authorization server a flow runs against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Idp {
    Github,
    Google,
}

impl Idp {
    pub fn id(self) -> &'static str {
        match self {
            Idp::Github => "github",
            Idp::Google => "google",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "github" => Some(Idp::Github),
            "google" => Some(Idp::Google),
            _ => None,
        }
    }

    /// `identities.issuer` for users signing in through this server.
    pub fn issuer(self) -> &'static str {
        match self {
            Idp::Github => "https://github.com",
            Idp::Google => "https://accounts.google.com",
        }
    }

    pub fn client(self) -> Option<OAuthClient> {
        match self {
            Idp::Github => super::config::github(),
            Idp::Google => super::config::google(),
        }
    }

    /// The authorization server behind a connection provider.
    pub fn for_connection(provider: Provider) -> Option<Self> {
        match provider {
            Provider::Github => Some(Idp::Github),
            Provider::Gdrive => Some(Idp::Google),
            _ => None,
        }
    }

    fn authorize_endpoint(self) -> &'static str {
        match self {
            Idp::Github => "https://github.com/login/oauth/authorize",
            Idp::Google => "https://accounts.google.com/o/oauth2/v2/auth",
        }
    }

    fn token_endpoint(self) -> &'static str {
        match self {
            Idp::Github => "https://github.com/login/oauth/access_token",
            Idp::Google => "https://oauth2.googleapis.com/token",
        }
    }
}

/// What a round trip is for.
#[derive(Clone, Debug, PartialEq)]
pub enum Purpose {
    Login,
    Connect {
        user_id: String,
        org_id: String,
        provider: Provider,
    },
}

/// Scopes and extra authorization parameters per (server, purpose).
fn scopes(idp: Idp, purpose: &Purpose) -> (&'static str, &'static [(&'static str, &'static str)]) {
    match (idp, purpose) {
        (Idp::Github, Purpose::Login) => ("read:user user:email", &[]),
        (Idp::Github, Purpose::Connect { .. }) => ("read:user user:email repo", &[]),
        (Idp::Google, Purpose::Login) => ("openid email profile", &[]),
        // A refresh token is only issued with offline access, and only on a
        // consent screen — so force one, or a reconnect would get none.
        (Idp::Google, Purpose::Connect { .. }) => (
            "openid email https://www.googleapis.com/auth/drive",
            &[("access_type", "offline"), ("prompt", "consent")],
        ),
    }
}

pub fn redirect_uri(public_url: &str, idp: Idp) -> String {
    format!("{public_url}/auth/{}/callback", idp.id())
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Record a new round trip and return the provider URL to send the browser to.
pub async fn start(
    idp: Idp,
    client: &OAuthClient,
    purpose: Purpose,
    public_url: &str,
) -> Result<String, ServerFnError> {
    let state = random_token(32)?;
    let verifier = random_token(32)?;
    let (user_id, org_id, provider) = match &purpose {
        Purpose::Login => (None, None, None),
        Purpose::Connect {
            user_id,
            org_id,
            provider,
        } => (
            Some(user_id.clone()),
            Some(org_id.clone()),
            Some(provider.id()),
        ),
    };
    sqlx::query(
        "insert into oauth_flows \
         (state, idp, purpose, pkce_verifier, user_id, org_id, connection_provider, expires_at) \
         values ($1, $2, $3, $4, $5::uuid, $6::uuid, $7, now() + interval '10 minutes')",
    )
    .bind(&state)
    .bind(idp.id())
    .bind(if purpose == Purpose::Login {
        "login"
    } else {
        "connect"
    })
    .bind(&verifier)
    .bind(user_id)
    .bind(org_id)
    .bind(provider)
    .execute(pool()?)
    .await
    .map_err(db_error)?;

    let (scope, extra) = scopes(idp, &purpose);
    let challenge = pkce_challenge(&verifier);
    let redirect = redirect_uri(public_url, idp);
    let mut params: Vec<(&str, &str)> = vec![
        ("client_id", client.id.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("response_type", "code"),
        ("scope", scope),
        ("state", state.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ];
    params.extend_from_slice(extra);
    let url = url::Url::parse_with_params(idp.authorize_endpoint(), &params)
        .expect("the authorize endpoints are valid URLs");
    Ok(url.into())
}

/// A resumed round trip.
pub struct Flow {
    pub purpose: Purpose,
    pub verifier: String,
}

/// Consume the flow for `state`, if it exists, is unexpired, and belongs to
/// `idp` (a GitHub callback cannot resume a Google flow).
pub async fn take(state: &str, idp: Idp) -> Result<Option<Flow>, ServerFnError> {
    let row = sqlx::query(
        "delete from oauth_flows where state = $1 \
         returning idp, purpose, pkce_verifier, user_id::text as user_id, \
                   org_id::text as org_id, connection_provider, expires_at > now() as fresh",
    )
    .bind(state)
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    let Some(row) = row else { return Ok(None) };
    if !row.get::<bool, _>("fresh") || row.get::<String, _>("idp") != idp.id() {
        return Ok(None);
    }
    let purpose = if row.get::<String, _>("purpose") == "login" {
        Purpose::Login
    } else {
        let provider: Option<String> = row.get("connection_provider");
        match (
            row.get("user_id"),
            row.get("org_id"),
            provider.as_deref().and_then(Provider::from_id),
        ) {
            (Some(user_id), Some(org_id), Some(provider)) => Purpose::Connect {
                user_id,
                org_id,
                provider,
            },
            _ => return Ok(None),
        }
    };
    Ok(Some(Flow {
        purpose,
        verifier: row.get("pkce_verifier"),
    }))
}

/// A token response.
#[derive(Deserialize)]
pub struct Tokens {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// Exchange an authorization code. Errors are short, credential-free strings
/// fit to show the user.
pub async fn exchange(
    idp: Idp,
    client: &OAuthClient,
    code: &str,
    verifier: &str,
    public_url: &str,
) -> Result<Tokens, String> {
    let redirect = redirect_uri(public_url, idp);
    let form = [
        ("client_id", client.id.as_str()),
        ("client_secret", client.secret.as_str()),
        ("code", code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect.as_str()),
        ("grant_type", "authorization_code"),
    ];
    let response = http()
        .post(idp.token_endpoint())
        .header("accept", "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|e| {
            eprintln!("{} token exchange failed: {e}", idp.id());
            "could not reach the provider".to_string()
        })?;
    // GitHub reports errors with a 200; Google with a 4xx. Read the body either way.
    let tokens: Tokens = response.json().await.map_err(|e| {
        eprintln!("{} token response unreadable: {e}", idp.id());
        "the provider's answer was unreadable".to_string()
    })?;
    if let Some(error) = &tokens.error {
        let detail = tokens.error_description.as_deref().unwrap_or("");
        eprintln!("{} token exchange refused: {error} {detail}", idp.id());
        return Err(format!("the provider refused the sign-in ({error})"));
    }
    if tokens.access_token.is_none() {
        return Err("the provider returned no access token".to_string());
    }
    Ok(tokens)
}

async fn get_json<T: for<'de> Deserialize<'de>>(url: &str, token: &str) -> Result<T, String> {
    let response = http()
        .get(url)
        .bearer_auth(token)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            eprintln!("GET {url} failed: {e}");
            "could not reach the provider".to_string()
        })?;
    if !response.status().is_success() {
        eprintln!("GET {url}: {}", response.status());
        return Err(format!(
            "the provider answered {}",
            response.status().as_u16()
        ));
    }
    response.json().await.map_err(|e| {
        eprintln!("GET {url}: unreadable body: {e}");
        "the provider's answer was unreadable".to_string()
    })
}

/// A GitHub identity, plus the login (used to label a `github` connection).
pub async fn github_identity(token: &str) -> Result<(ProviderIdentity, String), String> {
    #[derive(Deserialize)]
    struct GhUser {
        id: u64,
        login: String,
        name: Option<String>,
    }
    #[derive(Deserialize)]
    struct GhEmail {
        email: String,
        primary: bool,
        verified: bool,
    }
    let user: GhUser = get_json("https://api.github.com/user", token).await?;
    let emails: Vec<GhEmail> = get_json("https://api.github.com/user/emails", token).await?;
    let email = emails
        .into_iter()
        .find(|e| e.primary && e.verified)
        .map(|e| e.email)
        .ok_or("your GitHub account has no verified primary email")?;
    let identity = ProviderIdentity {
        issuer: Idp::Github.issuer(),
        subject: user.id.to_string(),
        email,
        display_name: user
            .name
            .filter(|n| !n.trim().is_empty())
            .or(Some(user.login.clone())),
    };
    Ok((identity, user.login))
}

/// A Google identity, from the OIDC userinfo endpoint. The access token came
/// straight from Google's token endpoint over TLS, so asking Google who it
/// belongs to is as strong as verifying the id_token, and needs no JWKS.
pub async fn google_identity(token: &str) -> Result<ProviderIdentity, String> {
    #[derive(Deserialize)]
    struct UserInfo {
        sub: String,
        email: Option<String>,
        email_verified: Option<bool>,
        name: Option<String>,
    }
    let info: UserInfo =
        get_json("https://openidconnect.googleapis.com/v1/userinfo", token).await?;
    let email = match (info.email, info.email_verified) {
        (Some(email), Some(true)) => email,
        _ => return Err("your Google account has no verified email".to_string()),
    };
    Ok(ProviderIdentity {
        issuer: Idp::Google.issuer(),
        subject: info.sub,
        email,
        display_name: info.name.filter(|n| !n.trim().is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7636 appendix B.
    #[test]
    fn pkce_s256_matches_the_rfc() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn drive_connect_asks_for_offline_access() {
        let purpose = Purpose::Connect {
            user_id: "u".into(),
            org_id: "o".into(),
            provider: Provider::Gdrive,
        };
        let (scope, extra) = scopes(Idp::Google, &purpose);
        assert!(scope.contains("auth/drive"));
        assert!(extra.contains(&("access_type", "offline")));
        assert!(extra.contains(&("prompt", "consent")));
        assert_eq!(
            scopes(Idp::Google, &Purpose::Login).0,
            "openid email profile"
        );
    }

    #[test]
    fn callback_paths() {
        assert_eq!(
            redirect_uri("https://a.b", Idp::Github),
            "https://a.b/auth/github/callback"
        );
        assert_eq!(
            redirect_uri("https://a.b", Idp::Google),
            "https://a.b/auth/google/callback"
        );
    }
}
