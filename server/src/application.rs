use std::sync::Arc;

use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_session::{DatabasePool, Session, SessionConfig, SessionLayer, SessionStore};
use axum_session_auth::{AuthConfig, AuthSession, AuthSessionLayer, Authentication, HasPermission};
use axum_session_sqlx::SessionPgPool;
use dioxus::prelude::*;
use dioxus_cli_config;
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, RedirectUrl, TokenUrl,
};
use serde::{Deserialize, Serialize};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions, PgPool,
};
use std::net::SocketAddr;
use time::Duration;

use super::{auth::oauth_client, database::connection_pool, settings::Settings, user::User};

#[derive(Deserialize)]
struct AuthCallback {
    code: String,
    state: String,
}

// App state
#[derive(Clone)]
pub struct AppState {
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
            let oauth_client = Arc::new(oauth_client(&settings));
            // Create session layer
            let session_config = SessionConfig::default().with_table_name("sessions");
            let session_store = SessionStore::<SessionPgPool>::new(
                Some(SessionPgPool::from(pool.clone())),
                session_config,
            )
            .await
            .expect("Cannot create a session store");
            let session_layer = SessionLayer::new(session_store);
            // Create an auth session layer
            let auth_config = AuthConfig::<i32>::default().with_anonymous_user_id(Some(1));
            let auth_session_layer =
                AuthSessionLayer::<User, i32, SessionPgPool, PgPool>::new(Some(pool))
                    .with_config(auth_config);
            // Get the address the server should run on.
            let addr = dioxus_cli_config::fullstack_address_or_localhost();
            // Build a config
            let serve_config = ServeConfig::new().unwrap();
            // Build our application with some routes
            let router = Router::new()
                .serve_dioxus_application(serve_config, app)
                .layer(auth_session_layer)
                .layer(session_layer)
                .into_make_service();
            // Run it
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .expect("Listener failure");
            axum::serve(listener, router).await.unwrap();
        });
}
