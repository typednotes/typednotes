//! # Google OAuth 2.0 implementation
//!
//! Implements the full Google Authorization Code flow with PKCE for TypedNotes.
//! The structure mirrors [`super::github`] but targets Google's endpoints and scopes.
//!
//! ## Types
//!
//! - [`GoogleUser`] — deserialization target for the Google userinfo API response
//!   (`googleapis.com/oauth2/v2/userinfo`).
//! - [`ConfiguredClient`] — a fully-typed `oauth2::Client` alias with auth and token
//!   endpoints set.
//! - [`GoogleOAuth`] — the public handler that wraps an [`OAuthConfig`].
//!
//! ## Flow
//!
//! 1. **[`generate_auth_url`](GoogleOAuth::generate_auth_url)** — builds an authorization
//!    URL requesting `openid`, `email`, and `profile` scopes, generates a random PKCE
//!    challenge, and persists the CSRF state + verifier in the `oauth_states` table with
//!    a 10-minute expiry.
//!
//! 2. **[`exchange_code`](GoogleOAuth::exchange_code)** — called by the `/auth/google/callback`
//!    route in the `web` crate. It:
//!    - Retrieves and atomically deletes the matching `oauth_states` row (validating CSRF
//!      state and expiry in one query).
//!    - Exchanges the authorization code + PKCE verifier for an access token.
//!    - Fetches the user's profile from the Google userinfo endpoint.
//!    - Upserts the user in the `users` table (keyed on `provider = 'google'` +
//!      `provider_id`) so returning users get their profile refreshed.

use oauth2::basic::BasicClient;
use oauth2::{
    AuthorizationCode, CsrfToken, EndpointNotSet, EndpointSet, PkceCodeChallenge,
    PkceCodeVerifier, Scope, TokenResponse,
};
use reqwest::Client;
use serde::Deserialize;

use super::config::OAuthConfig;
use crate::db::get_pool;
use crate::models::User;

/// Google user info from API.
#[derive(Debug, Deserialize)]
struct GoogleUser {
    id: String,
    email: String,
    name: Option<String>,
    picture: Option<String>,
}

/// OAuth client type with auth URL and token URL set.
type ConfiguredClient = oauth2::Client<
    oauth2::basic::BasicErrorResponse,
    oauth2::basic::BasicTokenResponse,
    oauth2::basic::BasicTokenIntrospectionResponse,
    oauth2::StandardRevocableToken,
    oauth2::basic::BasicRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;

/// Google OAuth handler.
pub struct GoogleOAuth {
    config: OAuthConfig,
}

impl GoogleOAuth {
    /// Create a new Google OAuth handler.
    pub fn new() -> Result<Self, String> {
        let config = OAuthConfig::google()?;
        Ok(Self { config })
    }

    fn create_client(&self) -> ConfiguredClient {
        BasicClient::new(self.config.client_id.clone())
            .set_client_secret(self.config.client_secret.clone())
            .set_auth_uri(self.config.auth_url.clone())
            .set_token_uri(self.config.token_url.clone())
            .set_redirect_uri(self.config.redirect_url.clone())
    }

    /// Generate authorization URL with PKCE.
    pub async fn generate_auth_url(&self) -> Result<(String, String, String), String> {
        let client = self.create_client();
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, csrf_state) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        // Store state and verifier in database
        let pool = get_pool().await.map_err(|e| e.to_string())?;
        let state = csrf_state.secret().clone();
        let verifier = pkce_verifier.secret().clone();

        sqlx::query(
            r#"
            INSERT INTO oauth_states (state, provider, pkce_verifier, expires_at)
            VALUES ($1, 'google', $2, NOW() + INTERVAL '10 minutes')
            "#,
        )
        .bind(&state)
        .bind(&verifier)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok((auth_url.to_string(), state, verifier))
    }

    /// Exchange authorization code for tokens and get user info.
    pub async fn exchange_code(
        &self,
        code: &str,
        state: &str,
    ) -> Result<User, String> {
        let pool = get_pool().await.map_err(|e| e.to_string())?;

        // Retrieve and delete the state from database
        let row: Option<(String,)> = sqlx::query_as(
            r#"
            DELETE FROM oauth_states
            WHERE state = $1 AND provider = 'google' AND expires_at > NOW()
            RETURNING pkce_verifier
            "#,
        )
        .bind(state)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

        let pkce_verifier = row
            .ok_or("Invalid or expired OAuth state")?
            .0;

        // Create HTTP client for token exchange
        let http_client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;

        let client = self.create_client();

        // Exchange code for token
        let token_result = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
            .request_async(&http_client)
            .await
            .map_err(|e| format!("Token exchange failed: {}", e))?;

        let access_token = token_result.access_token().secret();

        // Fetch user info from Google API
        let api_client = Client::new();

        let google_user: GoogleUser = api_client
            .get("https://www.googleapis.com/oauth2/v2/userinfo")
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        // Upsert user in database
        let user: User = sqlx::query_as(
            r#"
            INSERT INTO users (email, name, avatar_url, provider, provider_id)
            VALUES ($1, $2, $3, 'google', $4)
            ON CONFLICT (provider, provider_id)
            DO UPDATE SET
                email = EXCLUDED.email,
                name = EXCLUDED.name,
                avatar_url = EXCLUDED.avatar_url,
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(&google_user.email)
        .bind(&google_user.name)
        .bind(&google_user.picture)
        .bind(&google_user.id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(user)
    }
}
