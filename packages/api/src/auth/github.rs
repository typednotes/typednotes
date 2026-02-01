//! GitHub OAuth implementation.

use oauth2::basic::BasicClient;
use oauth2::{
    AuthorizationCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, Scope, TokenResponse,
};
use reqwest::Client;
use serde::Deserialize;

use super::config::OAuthConfig;
use crate::db::get_pool;
use crate::models::User;

/// GitHub user info from API.
#[derive(Debug, Deserialize)]
struct GitHubUser {
    id: i64,
    login: String,
    email: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
}

/// GitHub email info from API.
#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

/// GitHub OAuth handler.
pub struct GitHubOAuth {
    config: OAuthConfig,
}

impl GitHubOAuth {
    /// Create a new GitHub OAuth handler.
    pub fn new() -> Result<Self, String> {
        let config = OAuthConfig::github()?;
        Ok(Self { config })
    }

    fn create_client(&self) -> BasicClient {
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
            .add_scope(Scope::new("user:email".to_string()))
            .add_scope(Scope::new("read:user".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        // Store state and verifier in database
        let pool = get_pool().await.map_err(|e| e.to_string())?;
        let state = csrf_state.secret().clone();
        let verifier = pkce_verifier.secret().clone();

        sqlx::query(
            r#"
            INSERT INTO oauth_states (state, provider, pkce_verifier, expires_at)
            VALUES ($1, 'github', $2, NOW() + INTERVAL '10 minutes')
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
            WHERE state = $1 AND provider = 'github' AND expires_at > NOW()
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

        // Fetch user info from GitHub API
        let api_client = Client::new();

        let github_user: GitHubUser = api_client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {}", access_token))
            .header("User-Agent", "TypedNotes")
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        // Get primary email if not in user info
        let email = match github_user.email {
            Some(email) => email,
            None => {
                let emails: Vec<GitHubEmail> = api_client
                    .get("https://api.github.com/user/emails")
                    .header("Authorization", format!("Bearer {}", access_token))
                    .header("User-Agent", "TypedNotes")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .json()
                    .await
                    .map_err(|e| e.to_string())?;

                emails
                    .into_iter()
                    .find(|e| e.primary && e.verified)
                    .map(|e| e.email)
                    .ok_or("No verified primary email found")?
            }
        };

        // Upsert user in database
        let user: User = sqlx::query_as(
            r#"
            INSERT INTO users (email, name, avatar_url, provider, provider_id)
            VALUES ($1, $2, $3, 'github', $4)
            ON CONFLICT (provider, provider_id)
            DO UPDATE SET
                email = EXCLUDED.email,
                name = EXCLUDED.name,
                avatar_url = EXCLUDED.avatar_url,
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(&email)
        .bind(&github_user.name.or(Some(github_user.login)))
        .bind(&github_user.avatar_url)
        .bind(github_user.id.to_string())
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(user)
    }
}
