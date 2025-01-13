use super::{
    user::User,
    settings::Settings
};
use dioxus::prelude::server_fn::redirect;
use sqlx::PgPool;
use axum::{
    extract::State,
    http::{Method, status::StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use axum_session_auth::{AuthSession, AuthSessionLayer, Authentication, AuthConfig, HasPermission};
use axum_session_sqlx::SessionPgPool;
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, RedirectUrl, TokenUrl,
};
use async_trait::async_trait;

// https://github.com/DioxusLabs/dioxus/blob/v0.6/examples/fullstack-auth/src/auth.rs
// https://claude.ai/chat/e30efb82-bf2c-4e36-b7be-7bc6f7aece35


pub struct Session(
    pub AuthSession<User, i32, SessionPgPool, PgPool>,
);

impl std::ops::Deref for Session {
    type Target = AuthSession<User, i32, SessionPgPool, PgPool>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct AuthSessionLayerNotFound;

impl std::fmt::Display for AuthSessionLayerNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AuthSessionLayer was not found")
    }
}

impl std::error::Error for AuthSessionLayerNotFound {}

impl IntoResponse for AuthSessionLayerNotFound {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "AuthSessionLayer was not found",
        )
            .into_response()
    }
}

#[async_trait]
impl<S: std::marker::Sync + std::marker::Send> axum::extract::FromRequestParts<S> for Session {
    type Rejection = AuthSessionLayerNotFound;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        AuthSession::<User, i32, SessionPgPool, PgPool>::from_request_parts(parts, state)
        .await
        .map(Session)
        .map_err(|_| AuthSessionLayerNotFound)
    }
}

/// Initialize the OAuth client
pub fn oauth_client(settings: & Settings) -> BasicClient {
    let client_id = ClientId::new(settings.github.id.clone());
    let client_secret = ClientSecret::new(settings.github.secret.clone());
    let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
        .expect("Invalid authorization endpoint URL");
    let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
        .expect("Invalid token endpoint URL");
    let redirect_url = RedirectUrl::new(settings.auth.redirect.clone())
        .expect("Invalid redirect URL");

    BasicClient::new(
        client_id,
        Some(client_secret),
        auth_url,
        Some(token_url),
    ).set_redirect_uri(redirect_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_client() {
        let settings = Settings::new().unwrap_or_default();
        println!("Settings: {settings:?}");
        let client = oauth_client(&settings);
        println!("Oauth client: {client:?}");
    }
}