use dioxus::prelude::*;

use ui::AuthProvider;
use views::{Login, NoteDetail, Notes, Register, Settings, SidebarLayout};

mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Root {},
    #[route("/login")]
    Login {},
    #[route("/register")]
    Register {},
    #[layout(SidebarLayout)]
        #[route("/notes")]
        Notes {},
        #[route("/notes/:note_path")]
        NoteDetail { note_path: String },
        #[route("/settings")]
        Settings {},
}

const FAVICON: Asset = asset!("/assets/favicon.ico");

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

    // Run migrations (ignore versions already applied but missing from binary,
    // which can happen during rollbacks or when migrations are renumbered)
    let mut migrator = sqlx::migrate!("../api/migrations");
    migrator.set_ignore_missing(true);
    migrator
        .run(pool)
        .await
        .expect("Failed to run migrations");

    // Create session store
    let session_store = PostgresStore::new(pool.clone());

    // Detect production: IP=0.0.0.0 is set in container/production
    let is_production = std::env::var("IP")
        .map(|ip| ip == "0.0.0.0")
        .unwrap_or(false);

    // Session layer configuration
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(is_production)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(
            Duration::from_secs(60 * 60 * 24 * 7).try_into().unwrap(),
        )); // 7 days

    // Build the Dioxus app with custom routes
    let router = axum::Router::new()
        // Health check endpoint
        .route("/healthz", get(|| async { "ok" }))
        // Add custom OAuth callback routes first
        .route("/auth/github/callback", get(github_callback))
        .route("/auth/google/callback", get(google_callback))
        // Then serve the Dioxus application
        .serve_dioxus_application(ServeConfig::new(), App)
        // Add session layer to all routes
        .layer(session_layer);

    // Use IP/PORT env vars (set in production), falling back to dioxus default for local dev
    let addr = {
        let default = dioxus::cli_config::fullstack_address_or_localhost();
        let ip = std::env::var("IP")
            .ok()
            .and_then(|s| s.parse::<std::net::IpAddr>().ok())
            .unwrap_or(default.ip());
        let port = std::env::var("PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(default.port());
        std::net::SocketAddr::new(ip, port)
    };
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
    use_context_provider(|| Signal::new(ui::ActivityLog::default()));

    // Theme context: None = system, Some("dark"), Some("light")
    let mut theme: ui::ThemeSignal = use_context_provider(|| Signal::new(Option::<String>::None));
    // Load persisted theme from localStorage on startup
    use_effect(move || {
        ui::load_theme_from_storage(&mut theme);
    });

    rsx! {
        // Global app resources
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: ui::TAILWIND_CSS }
        document::Link { rel: "stylesheet", href: ui::DX_COMPONENTS_CSS }

        AuthProvider {
            ui::components::ToastProvider {
                Router::<Route> {}
            }
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
