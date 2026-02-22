use dioxus::prelude::*;

use ui::AuthProvider;
use views::{Login, NoteDetail, Notes, Settings};

mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Root {},
    #[route("/login")]
    Login {},
    #[route("/notes")]
    Notes {},
    #[route("/notes/:note_path")]
    NoteDetail { note_path: String },
    #[route("/settings")]
    Settings {},
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    #[cfg(feature = "server")]
    {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(launch_server());
    }

    #[cfg(not(feature = "server"))]
    {
        dioxus::launch(App);
    }
}

#[cfg(feature = "server")]
async fn launch_server() {
    use axum::routing::get;
    use dioxus::server::{DioxusRouterExt, ServeConfig};
    use std::time::Duration;
    use tower_sessions::cookie::SameSite;
    use tower_sessions::{Expiry, SessionManagerLayer};
    use tower_sessions_sqlx_store::PostgresStore;

    dotenvy::dotenv().ok();

    // Initialize database pool
    let pool = api::db::get_pool()
        .await
        .expect("Failed to connect to database");

    // Run migrations
    sqlx::migrate!("../api/migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");

    // Create session store
    let session_store = PostgresStore::new(pool.clone());

    // Session layer configuration
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // Set to true in production with HTTPS
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(
            Duration::from_secs(60 * 60 * 24 * 7).try_into().unwrap(),
        )); // 7 days

    // Build the Dioxus app with custom routes
    let router = axum::Router::new()
        // Add custom OAuth callback routes first
        .route("/auth/github/callback", get(github_callback))
        .route("/auth/google/callback", get(google_callback))
        // Then serve the Dioxus application
        .serve_dioxus_application(ServeConfig::new(), App)
        // Add session layer to all routes
        .layer(session_layer);

    // Use the address from dx serve or default to localhost:8080
    let addr = dioxus::cli_config::fullstack_address_or_localhost();
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, router.into_make_service())
        .await
        .unwrap();
}

#[cfg(feature = "server")]
async fn github_callback(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    session: tower_sessions::Session,
) -> axum::response::Redirect {
    use axum::response::Redirect;

    let Some(code) = params.get("code") else {
        tracing::error!("GitHub callback missing code");
        return Redirect::to("/login?error=missing_code");
    };
    let Some(state) = params.get("state") else {
        tracing::error!("GitHub callback missing state");
        return Redirect::to("/login?error=missing_state");
    };

    match api::auth::GitHubOAuth::new() {
        Ok(oauth) => match oauth.exchange_code(code, state).await {
            Ok(user) => {
                if let Err(e) = session
                    .insert(api::auth::SESSION_USER_ID_KEY, user.id.to_string())
                    .await
                {
                    tracing::error!("Failed to set session: {}", e);
                    return Redirect::to("/login?error=session_error");
                }
                Redirect::to("/notes")
            }
            Err(e) => {
                tracing::error!("GitHub OAuth error: {}", e);
                Redirect::to("/login?error=oauth_error")
            }
        },
        Err(e) => {
            tracing::error!("Failed to create GitHub OAuth: {}", e);
            Redirect::to("/login?error=config_error")
        }
    }
}

#[cfg(feature = "server")]
async fn google_callback(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    session: tower_sessions::Session,
) -> axum::response::Redirect {
    use axum::response::Redirect;

    let Some(code) = params.get("code") else {
        tracing::error!("Google callback missing code");
        return Redirect::to("/login?error=missing_code");
    };
    let Some(state) = params.get("state") else {
        tracing::error!("Google callback missing state");
        return Redirect::to("/login?error=missing_state");
    };

    match api::auth::GoogleOAuth::new() {
        Ok(oauth) => match oauth.exchange_code(code, state).await {
            Ok(user) => {
                if let Err(e) = session
                    .insert(api::auth::SESSION_USER_ID_KEY, user.id.to_string())
                    .await
                {
                    tracing::error!("Failed to set session: {}", e);
                    return Redirect::to("/login?error=session_error");
                }
                if let Err(e) = session.save().await {
                    tracing::error!("Failed to save session: {}", e);
                    return Redirect::to("/login?error=session_save_error");
                }
                Redirect::to("/notes")
            }
            Err(e) => {
                tracing::error!("Google OAuth exchange error: {}", e);
                Redirect::to("/login?error=oauth_error")
            }
        },
        Err(e) => {
            tracing::error!("Failed to create Google OAuth: {}", e);
            Redirect::to("/login?error=config_error")
        }
    }
}

#[component]
fn App() -> Element {
    rsx! {
        // Global app resources
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        AuthProvider {
            Router::<Route> {}
        }
    }
}

/// Redirect `/` to `/notes`
#[component]
fn Root() -> Element {
    let nav = use_navigator();
    nav.replace(Route::Notes {});
    rsx! {}
}
