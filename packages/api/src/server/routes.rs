//! Plain HTTP routes for the OAuth round trips (docs/connections.md §2–3).
//! These are browser navigations — redirects out to a provider and back with
//! a cookie — not server functions, so they are axum handlers merged into
//! the Dioxus router (`web`'s `main`).
//!
//! - `GET /auth/{github|google}/login`
//! - `GET /auth/connect/{github|gdrive}?org={slug}`
//! - `GET /auth/{github|google}/callback?code=…&state=…`
//!
//! Failures redirect back to a page with `?error=…` rather than rendering an
//! error body: the message is short, credential-free, and shown in the UI.

use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::fullstack::HeaderMap;
use dioxus::server::axum::extract::{Path, Query};
use dioxus::server::axum::http::header::SET_COOKIE;
use dioxus::server::axum::response::{IntoResponse, Redirect, Response};
use dioxus::server::axum::routing::get;
use dioxus::server::axum::Router;
use serde::Deserialize;

use super::oauth::{self, Idp, Purpose};
use super::{config, connections, db, session, vault};
use crate::{Provider, User};

pub fn router() -> Router {
    Router::new()
        .route("/auth/{idp}/login", get(login))
        .route("/auth/{idp}/callback", get(callback))
        .route("/auth/connect/{provider}", get(connect))
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

async fn login(Path(idp): Path<String>, headers: HeaderMap) -> Response {
    let Some(idp) = Idp::from_id(&idp) else {
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
    let Some((provider, idp)) =
        Provider::from_id(&provider).and_then(|p| Some((p, Idp::for_connection(p)?)))
    else {
        return to_org_error(&org.slug, "this provider is not connected through OAuth");
    };
    let Some(client) = idp.client() else {
        return to_org_error(&org.slug, &format!("{} is not configured", provider.name()));
    };
    if !vault::configured() {
        return to_org_error(
            &org.slug,
            "connections are disabled: the vault is not configured",
        );
    }
    let purpose = Purpose::Connect {
        user_id: user.id,
        org_id: org.id,
        provider,
    };
    match oauth::start(idp, &client, purpose, &config::public_url(&headers)).await {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(e) => to_org_error(&org.slug, &e.to_string()),
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
    let fail = |message: &str| match &org {
        None => to_login(message),
        Some(org) => to_org_error(&org.slug, message),
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
            let org = org.expect("connect flows resolved their org above");
            // The browser finishing the round trip must be the member who
            // started it — otherwise a victim could be made to attach an
            // attacker's account (or the reverse) through a crafted link.
            let user = match session::user_for(&headers).await {
                Ok(Some(user)) if user.id == user_id => user,
                _ => return to_login("sign in as the member who started this connection"),
            };
            match finish_connect(&org, &user, provider, &tokens, &access_token).await {
                Ok(()) => to_org(&org.slug, &format!("connected={}", provider.id())),
                Err(message) => to_org_error(&org.slug, &message),
            }
        }
    }
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
    let (label, credential) = match provider {
        Provider::Github => {
            let (_, login) = oauth::github_identity(access_token).await?;
            (format!("@{login}"), vault::bearer(base_url, access_token))
        }
        Provider::Gdrive => {
            let refresh = tokens
                .refresh_token
                .as_deref()
                .ok_or("Google returned no refresh token — remove Typednotes from your Google account's third-party access and connect again")?;
            let email = oauth::google_identity(access_token)
                .await
                .map(|i| i.email)?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let expires_at = now + tokens.expires_in.unwrap_or(3600).max(0) as u64;
            (
                email,
                vault::google_oauth(base_url, access_token, refresh, expires_at),
            )
        }
        _ => return Err("this provider is not connected through OAuth".to_string()),
    };
    connections::store(org, user, provider, &label, base_url, credential)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}
