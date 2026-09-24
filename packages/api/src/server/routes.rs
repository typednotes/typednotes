//! Plain HTTP routes for the OAuth round trips (docs/connections.md §2–3).
//! These are browser navigations — redirects out to a provider and back with
//! a cookie — not server functions, so they are axum handlers merged into
//! the Dioxus router (`web`'s `main`).
//!
//! - `GET /auth/{github|google}/login`
//! - `GET /auth/connect/{github|gitlab|gdrive|dropbox|slack}?org={slug}[&project={slug}]`
//! - `GET /auth/{github|google|gitlab|dropbox|slack}/callback?code=…&state=…`
//!
//! Failures redirect back to a page with `?error=…` rather than rendering an
//! error body: the message is short, credential-free, and shown in the UI.
//!
//! And the messaging webhooks (docs/connections.md §11), authenticated by
//! the providers' signatures:
//!
//! - `POST /hooks/slack` — Slack's Events API
//! - `GET|POST /hooks/whatsapp` — Meta's webhook verification and deliveries

use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::fullstack::HeaderMap;
use dioxus::server::axum::body::Bytes;
use dioxus::server::axum::extract::{Path, Query};
use dioxus::server::axum::http::header::{CONTENT_TYPE, SET_COOKIE};
use dioxus::server::axum::http::StatusCode;
use dioxus::server::axum::response::{IntoResponse, Redirect, Response};
use dioxus::server::axum::routing::{get, post};
use dioxus::server::axum::Router;
use serde::Deserialize;

use super::channels::{self, SlackEvent};
use super::connections::NewConnection;
use super::oauth::{self, Idp, Purpose};
use super::vault::OAuthIssuer;
use super::{config, connections, db, projects, session, vault};
use crate::{Provider, User};

pub fn router() -> Router {
    Router::new()
        .route("/auth/{idp}/login", get(login))
        .route("/auth/{idp}/callback", get(callback))
        .route("/auth/connect/{provider}", get(connect))
        .route("/hooks/slack", post(slack_hook))
        .route("/hooks/whatsapp", get(whatsapp_verify).post(whatsapp_hook))
}

/// Percent-encode a query value with `%20` for spaces: the Dioxus router
/// percent-decodes query arguments but does not turn `+` into a space.
/// (`byte_serialize` encodes a literal `+` as `%2B`, so every `+` it emits
/// stands for a space.)
fn encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

#[cfg(test)]
mod tests {
    #[test]
    fn query_values_use_percent_20() {
        assert_eq!(super::encode("a b+c/é"), "a%20b%2Bc%2F%C3%A9");
    }
}

fn to_login(message: &str) -> Response {
    Redirect::to(&format!("/login?error={}", encode(message))).into_response()
}

fn to_org(slug: &str, query: &str) -> Response {
    Redirect::to(&format!("/orgs/{}?{query}", encode(slug))).into_response()
}

fn to_org_error(slug: &str, message: &str) -> Response {
    to_org(slug, &format!("error={}", encode(message)))
}

/// A project page (`/orgs/{org}/projects/{project}`), with a query.
fn to_path(path: &str, query: &str) -> Response {
    Redirect::to(&format!("{path}?{query}")).into_response()
}

/// Where a connect flow comes back to: the project page it started from,
/// else the org page.
fn back(slug: &str, return_to: Option<&str>, query: &str) -> Response {
    match return_to {
        Some(path) => to_path(path, query),
        None => to_org(slug, query),
    }
}

fn project_path(org_slug: &str, project_slug: &str) -> String {
    format!(
        "/orgs/{}/projects/{}",
        encode(org_slug),
        encode(project_slug)
    )
}

async fn login(Path(idp): Path<String>, headers: HeaderMap) -> Response {
    let Some(idp) = Idp::from_id(&idp).filter(|idp| idp.signs_in()) else {
        return to_login("unknown sign-in provider");
    };
    let Some(client) = idp.client() else {
        return to_login(&format!("sign-in with {} is not configured", idp.id()));
    };
    match oauth::start(idp, &client, Purpose::Login, &config::public_url(&headers)).await {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(e) => to_login(&e.to_string()),
    }
}

#[derive(Deserialize)]
struct ConnectQuery {
    org: String,
    /// The project page the flow starts from, to come back to.
    project: Option<String>,
}

async fn connect(
    Path(provider): Path<String>,
    Query(q): Query<ConnectQuery>,
    headers: HeaderMap,
) -> Response {
    let Ok(Some(user)) = session::user_for(&headers).await else {
        return to_login("sign in to connect an account");
    };
    let Ok(Some(org)) = db::org_for_member(&q.org, &user.id).await else {
        return Redirect::to("/").into_response();
    };
    // Only a project of this org is a place to come back to.
    let return_to = match q.project.as_deref().filter(|p| !p.is_empty()) {
        None => None,
        Some(project) => match projects::get(&org, project).await {
            Ok((project, _)) => Some(project_path(&org.slug, &project.slug)),
            Err(_) => return to_org_error(&org.slug, "no such project"),
        },
    };
    let fail = |message: &str| {
        back(
            &org.slug,
            return_to.as_deref(),
            &format!("error={}", encode(message)),
        )
    };
    let Some((provider, idp)) =
        Provider::from_id(&provider).and_then(|p| Some((p, Idp::for_connection(p)?)))
    else {
        return fail("this provider is not connected through OAuth");
    };
    let Some(client) = idp.client() else {
        return fail(&format!("{} is not configured", provider.name()));
    };
    if !vault::configured() {
        return fail("connections are disabled: the vault is not configured");
    }
    let purpose = Purpose::Connect {
        user_id: user.id,
        org_id: org.id.clone(),
        provider,
        return_to: return_to.clone(),
    };
    match oauth::start(idp, &client, purpose, &config::public_url(&headers)).await {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(e) => fail(&e.to_string()),
    }
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn callback(
    Path(idp): Path<String>,
    Query(q): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(idp) = Idp::from_id(&idp) else {
        return to_login("unknown sign-in provider");
    };
    let flow = match q.state.as_deref() {
        Some(state) => oauth::take(state, idp).await.ok().flatten(),
        None => None,
    };
    let Some(flow) = flow else {
        return to_login("that sign-in link expired or was already used — try again");
    };

    // Where to send the browser if anything below fails.
    let org = match &flow.purpose {
        Purpose::Login => None,
        Purpose::Connect {
            user_id, org_id, ..
        } => match db::org_by_id_for_member(org_id, user_id).await {
            Ok(Some(org)) => Some(org),
            _ => return Redirect::to("/").into_response(),
        },
    };
    let return_to = match &flow.purpose {
        Purpose::Connect { return_to, .. } => return_to.clone(),
        Purpose::Login => None,
    };
    let fail = |message: &str| match &org {
        None => to_login(message),
        Some(org) => back(
            &org.slug,
            return_to.as_deref(),
            &format!("error={}", encode(message)),
        ),
    };

    if let Some(error) = q.error.as_deref() {
        return fail(&format!("the provider did not grant access ({error})"));
    }
    let (Some(code), Some(client)) = (q.code.as_deref(), idp.client()) else {
        return fail("the provider returned no authorization code");
    };
    let public_url = config::public_url(&headers);
    let tokens = match oauth::exchange(idp, &client, code, &flow.verifier, &public_url).await {
        Ok(tokens) => tokens,
        Err(message) => return fail(&message),
    };
    let access_token = tokens.access_token.clone().unwrap_or_default();

    match flow.purpose {
        Purpose::Login => {
            let identity = match idp {
                Idp::Github => oauth::github_identity(&access_token)
                    .await
                    .map(|(id, _)| id),
                Idp::Google => oauth::google_identity(&access_token).await,
                _ => Err("this provider does not sign in".to_string()),
            };
            let identity = match identity {
                Ok(identity) => identity,
                Err(message) => return fail(&message),
            };
            let user_id = match session::user_for_identity(&identity).await {
                Ok(id) => id,
                Err(e) => return fail(&e.to_string()),
            };
            match session::create(&user_id).await {
                Ok(token) => {
                    let cookie = session::set_cookie(&token, public_url.starts_with("https://"));
                    ([(SET_COOKIE, cookie)], Redirect::to("/")).into_response()
                }
                Err(e) => fail(&e.to_string()),
            }
        }
        Purpose::Connect {
            user_id, provider, ..
        } => {
            let org = org.clone().expect("connect flows resolved their org above");
            // The browser finishing the round trip must be the member who
            // started it — otherwise a victim could be made to attach an
            // attacker's account (or the reverse) through a crafted link.
            let user = match session::user_for(&headers).await {
                Ok(Some(user)) if user.id == user_id => user,
                _ => return to_login("sign in as the member who started this connection"),
            };
            match finish_connect(&org, &user, provider, &tokens, &access_token).await {
                Ok(()) => back(
                    &org.slug,
                    return_to.as_deref(),
                    &format!("connected={}", provider.id()),
                ),
                Err(message) => fail(&message),
            }
        }
    }
}

/// Seconds since the epoch, `expires_in` from now.
fn expiry(expires_in: Option<i64>) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now + expires_in.unwrap_or(3600).max(0) as u64
}

async fn finish_connect(
    org: &crate::Org,
    user: &User,
    provider: Provider,
    tokens: &oauth::Tokens,
    access_token: &str,
) -> Result<(), String> {
    let base_url = provider
        .fixed_base_url()
        .expect("OAuth providers have a fixed base URL");
    let refresh = |name: &str| {
        tokens.refresh_token.clone().ok_or(format!(
            "{name} returned no refresh token — remove Typednotes from your {name} account's \
             connected apps and connect again"
        ))
    };
    let mut external_id = None;
    let (label, credential) = match provider {
        Provider::Github => {
            let (_, login) = oauth::github_identity(access_token).await?;
            (format!("@{login}"), vault::bearer(base_url, access_token))
        }
        Provider::Gitlab => {
            let refresh = refresh("GitLab")?;
            let username = oauth::gitlab_username(access_token).await?;
            (
                format!("@{username}"),
                vault::oauth(
                    OAuthIssuer::Gitlab,
                    base_url,
                    access_token,
                    &refresh,
                    expiry(tokens.expires_in),
                ),
            )
        }
        Provider::Gdrive => {
            let refresh = refresh("Google")?;
            let email = oauth::google_identity(access_token)
                .await
                .map(|i| i.email)?;
            (
                email,
                vault::oauth(
                    OAuthIssuer::Google,
                    base_url,
                    access_token,
                    &refresh,
                    expiry(tokens.expires_in),
                ),
            )
        }
        Provider::Dropbox => {
            let refresh = refresh("Dropbox")?;
            let email = oauth::dropbox_email(access_token).await?;
            (
                email,
                vault::oauth(
                    OAuthIssuer::Dropbox,
                    base_url,
                    access_token,
                    &refresh,
                    expiry(tokens.expires_in),
                ),
            )
        }
        Provider::Slack => {
            // A bot token: it does not expire unless the Slack app turns on
            // token rotation, which this connection does not support.
            if tokens.refresh_token.is_some() {
                return Err(
                    "the Slack app has token rotation on; turn it off and connect again"
                        .to_string(),
                );
            }
            let team = tokens
                .team
                .as_ref()
                .ok_or("Slack did not say which workspace the app was installed in")?;
            external_id = Some(team.id.clone());
            (
                team.name.clone().unwrap_or_else(|| team.id.clone()),
                vault::bearer(base_url, access_token),
            )
        }
        _ => return Err("this provider is not connected through OAuth".to_string()),
    };
    connections::store(
        org,
        user,
        NewConnection {
            provider,
            label: &label,
            base_url,
            external_id: external_id.as_deref(),
        },
        credential,
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

// ── Messaging webhooks ──────────────────────────────────────────────────

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Slack's Events API. The signature is checked on the raw body before it
/// is parsed. Deliveries are acknowledged with `200` even when no project
/// routes them: Slack retries anything else, and would disable the app.
async fn slack_hook(headers: HeaderMap, body: Bytes) -> Response {
    let Some(secret) = config::slack_signing_secret() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "SLACK_SIGNING_SECRET is not set",
        )
            .into_response();
    };
    if !channels::slack_signature_ok(
        &secret,
        header(&headers, "x-slack-request-timestamp"),
        &body,
        header(&headers, "x-slack-signature"),
        now_secs(),
    ) {
        return (StatusCode::UNAUTHORIZED, "bad signature").into_response();
    }
    match channels::parse_slack_event(&body) {
        Ok(SlackEvent::UrlVerification(challenge)) => {
            ([(CONTENT_TYPE, "text/plain")], challenge).into_response()
        }
        Ok(SlackEvent::Message {
            team,
            channel,
            message,
        }) => {
            if let Err(e) =
                channels::deliver(Provider::Slack, &team, Some(&channel), &message).await
            {
                eprintln!("slack delivery failed: {e}");
                // A retry may succeed (the database was waking up).
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            StatusCode::OK.into_response()
        }
        Ok(SlackEvent::Ignored) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("unreadable event: {e}")).into_response(),
    }
}

#[derive(Deserialize)]
struct WhatsappVerify {
    #[serde(rename = "hub.mode")]
    mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    challenge: Option<String>,
}

/// Meta's webhook registration: echo the challenge if the verify token is
/// ours.
async fn whatsapp_verify(Query(q): Query<WhatsappVerify>) -> Response {
    let Some((_, expected)) = config::whatsapp_webhook() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "the WhatsApp webhook is not configured",
        )
            .into_response();
    };
    match (q.mode.as_deref(), q.verify_token, q.challenge) {
        (Some("subscribe"), Some(token), Some(challenge)) if token == expected => {
            ([(CONTENT_TYPE, "text/plain")], challenge).into_response()
        }
        _ => StatusCode::FORBIDDEN.into_response(),
    }
}

/// WhatsApp deliveries, signed with the Meta app's secret.
async fn whatsapp_hook(headers: HeaderMap, body: Bytes) -> Response {
    let Some((app_secret, _)) = config::whatsapp_webhook() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "the WhatsApp webhook is not configured",
        )
            .into_response();
    };
    if !channels::whatsapp_signature_ok(&app_secret, &body, header(&headers, "x-hub-signature-256"))
    {
        return (StatusCode::UNAUTHORIZED, "bad signature").into_response();
    }
    let messages = match channels::parse_whatsapp(&body) {
        Ok(messages) => messages,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("unreadable delivery: {e}")).into_response()
        }
    };
    for (phone_number_id, message) in &messages {
        if let Err(e) = channels::deliver(Provider::Whatsapp, phone_number_id, None, message).await
        {
            eprintln!("whatsapp delivery failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    StatusCode::OK.into_response()
}
