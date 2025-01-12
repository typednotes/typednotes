use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_cli_config;
use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use tower_sessions_sqlx_store::{sqlx::PgPool, PostgresStore};

use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, RedirectUrl, TokenUrl,
};
use serde::{Deserialize, Serialize};
use tower_sessions::{Session, SessionManager, SessionManagerLayer};

use super::{
    settings::Settings,
    database::connection_pool,
    auth::oauth_client,
};

#[derive(Deserialize)]
struct AuthCallback {
    code: String,
    state: String,
}

// App state
#[derive(Clone)]
struct AppState {
    pool: PgPool,
    oauth_client: Arc<BasicClient>,
}

/// Lanch a server with a session store for authentication
/// see https://crates.io/crates/dioxus-fullstack
pub fn launch(app: fn() -> Element) {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            // Get the settings
            let settings = Settings::new().unwrap_or_default();
            // Get the DB connection
            let pool = connection_pool(&settings).await.expect("Connect to the DB");
            // Initialize OAuth client
            let oauth_client = Arc::new(oauth_client(&settings)); // TODO this fails
            println!("Oauth client: {oauth_client:?}");
            // Create session layer
            let session_store = PostgresStore::new(pool.clone());
            let session_layer = SessionManagerLayer::new(session_store)
                .with_secure(false)  // Set to true in production
                .with_name("session");
            // Create app state
            let state = AppState {
                pool,
                oauth_client,
            };
            // Get the address the server should run on.
            let addr = dioxus_cli_config::fullstack_address_or_localhost();
            // Build our application with some routes
            let router = Router::new()
                .with_state(state)
                .layer(session_layer)
                .serve_dioxus_application(ServeConfigBuilder::default(), app)
                .into_make_service();
            
            // Run it
            let listener = tokio::net::TcpListener::bind(&addr).await.expect("Listener failure");
            axum::serve(listener, router)
                .await
                .unwrap();

        });
}

#[cfg(test)]
mod tests {
    use super::*;

}