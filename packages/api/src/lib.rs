//! This crate contains all shared fullstack server functions.

use dioxus::prelude::*;

pub mod auth;
pub mod db;
pub mod models;

pub use models::UserInfo;

/// Echo the user input on the server.
#[post("/api/echo")]
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    Ok(input)
}

/// Get the current authenticated user from the session.
#[cfg(feature = "server")]
#[get("/api/auth/me", session: tower_sessions::Session)]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    use crate::db::get_pool;
    use crate::models::User;

    let user_id: Option<String> = session
        .get(auth::SESSION_USER_ID_KEY)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some(user_id) = user_id else {
        return Ok(None);
    };

    let pool = get_pool()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let user_uuid = uuid::Uuid::parse_str(&user_id)
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_uuid)
        .fetch_optional(pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(user.map(|u| u.to_info()))
}

#[cfg(not(feature = "server"))]
#[get("/api/auth/me")]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    Ok(None)
}

/// Get the OAuth login URL for a provider.
#[cfg(feature = "server")]
#[get("/api/auth/login/:provider")]
pub async fn get_login_url(provider: String) -> Result<String, ServerFnError> {
    match provider.as_str() {
        "github" => {
            let oauth = auth::GitHubOAuth::new()
                .map_err(|e| ServerFnError::new(e))?;
            let (url, _, _) = oauth
                .generate_auth_url()
                .await
                .map_err(|e| ServerFnError::new(e))?;
            Ok(url)
        }
        "google" => {
            let oauth = auth::GoogleOAuth::new()
                .map_err(|e| ServerFnError::new(e))?;
            let (url, _, _) = oauth
                .generate_auth_url()
                .await
                .map_err(|e| ServerFnError::new(e))?;
            Ok(url)
        }
        _ => Err(ServerFnError::new(format!("Unknown provider: {}", provider))),
    }
}

#[cfg(not(feature = "server"))]
#[get("/api/auth/login/:provider")]
pub async fn get_login_url(provider: String) -> Result<String, ServerFnError> {
    Err(ServerFnError::new("Server only"))
}

/// Log out the current user by clearing the session.
#[cfg(feature = "server")]
#[post("/api/auth/logout", session: tower_sessions::Session)]
pub async fn logout() -> Result<(), ServerFnError> {
    session
        .flush()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(())
}

#[cfg(not(feature = "server"))]
#[post("/api/auth/logout")]
pub async fn logout() -> Result<(), ServerFnError> {
    Ok(())
}
