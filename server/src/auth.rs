use dioxus::prelude::server_fn::redirect;
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, RedirectUrl, TokenUrl,
};
use crate::settings;

use super::settings::Settings;

#[derive(sqlx::FromRow, Clone)]
pub struct SqlPermissionTokens {
    pub token: String,
}

/// Initialize the OAuth client
pub fn oauth_client(settings: & Settings) -> BasicClient {
    let client_id = ClientId::new(settings.github.client_id.clone());
    let client_secret = ClientSecret::new(settings.github.client_secret.clone());
    let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
        .expect("Invalid authorization endpoint URL");
    let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
        .expect("Invalid token endpoint URL");
    let redirect_url = RedirectUrl::new(settings.auth.redirect_url.clone())
        .expect("Invalid redirect URL");

    BasicClient::new(
        client_id,
        Some(client_secret),
        auth_url,
        Some(token_url),
    ).set_redirect_uri(redirect_url)
}