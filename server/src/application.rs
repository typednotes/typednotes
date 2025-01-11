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
use sqlx::{Pool, Postgres};
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
            let settings = Settings::new().expect("Read config");
            // Get the DB connection
            let pool = connection_pool(settings).await.expect("Connect to the DB");
            // Initialize OAuth client
            let oauth_client = Arc::new(oauth_client());
            // Create session layer
            let session_store = tower_sessions_sqlx_store::PostgresStore::new(pool.clone());
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
                // .with_state(state)
                // .layer(session_layer)
                .serve_dioxus_application(ServeConfigBuilder::default(), app)
                .into_make_service();
            
            // Run it
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            axum::serve(listener, router)
                .await
                .unwrap();

        });
}

// /// Lanch a server with a session store for authentication
// pub fn launch(app: fn() -> Element) {
//     tokio::runtime::Runtime::new()
//         .unwrap()
//         .block_on(async move {
//             // Create the session store
//             let session_store = PostgresStore::new(connection_pool().await.unwrap().clone());
//             session_store.migrate().await.unwrap();
//             // Create a tower layer
//             let session_layer = SessionManagerLayer::new(session_store)
//                 .with_secure(false)
//                 .with_expiry(Expiry::OnInactivity(Duration::seconds(10)));

//             // User::create_user_tables(&pool).await;

//             // build our application with some routes
//             let app = Router::new()
//                 // Server side render the application, serve static assets, and register server functions
//                 .serve_dioxus_application(ServeConfig::new().unwrap(), app)
//                 .layer(session_layer);

//             // run it
//             let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
//             let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

//             axum::serve(listener, app.into_make_service())
//                 .await
//                 .unwrap();
//         });
// }