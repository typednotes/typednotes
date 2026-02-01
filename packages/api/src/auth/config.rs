//! OAuth configuration from environment variables.

use oauth2::{AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};

/// OAuth provider configuration.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: ClientId,
    pub client_secret: ClientSecret,
    pub auth_url: AuthUrl,
    pub token_url: TokenUrl,
    pub redirect_url: RedirectUrl,
}

impl OAuthConfig {
    /// Create GitHub OAuth config from environment variables.
    pub fn github() -> Result<Self, String> {
        dotenvy::dotenv().ok();

        let client_id = std::env::var("GITHUB_CLIENT_ID")
            .map_err(|_| "GITHUB_CLIENT_ID not set")?;
        let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
            .map_err(|_| "GITHUB_CLIENT_SECRET not set")?;
        let redirect_uri = std::env::var("AUTH_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:8080/auth/github/callback".to_string());

        Ok(Self {
            client_id: ClientId::new(client_id),
            client_secret: ClientSecret::new(client_secret),
            auth_url: AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
                .map_err(|e| e.to_string())?,
            token_url: TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
                .map_err(|e| e.to_string())?,
            redirect_url: RedirectUrl::new(redirect_uri.replace("/callback", "/github/callback"))
                .map_err(|e| e.to_string())?,
        })
    }

    /// Create Google OAuth config from environment variables.
    pub fn google() -> Result<Self, String> {
        dotenvy::dotenv().ok();

        let client_id = std::env::var("GOOGLE_CLIENT_ID")
            .map_err(|_| "GOOGLE_CLIENT_ID not set")?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
            .map_err(|_| "GOOGLE_CLIENT_SECRET not set")?;
        let redirect_uri = std::env::var("AUTH_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:8080/auth/google/callback".to_string());

        Ok(Self {
            client_id: ClientId::new(client_id),
            client_secret: ClientSecret::new(client_secret),
            auth_url: AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
                .map_err(|e| e.to_string())?,
            token_url: TokenUrl::new("https://oauth2.googleapis.com/token".to_string())
                .map_err(|e| e.to_string())?,
            redirect_url: RedirectUrl::new(redirect_uri.replace("/callback", "/google/callback"))
                .map_err(|e| e.to_string())?,
        })
    }
}
