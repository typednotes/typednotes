use dioxus::prelude::*;
use axum::routing::*;
use time::Duration;
use tower_sessions::{Expiry, Session, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;
use super::database::connection_pool;



pub fn launch(app: fn() -> Element) {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            // Create the session store
            let session_store = PostgresStore::new(connection_pool().await?);
            session_store.migrate().await?;
            // Remove outdated entries
            let deletion_task = tokio::task::spawn(
                session_store
                    .clone()
                    .continuously_delete_expired(tokio::time::Duration::from_secs(60)),
            );
            // Create a tower layer
            let session_layer = SessionManagerLayer::new(session_store)
                .with_secure(false)
                .with_expiry(Expiry::OnInactivity(Duration::seconds(10)));

            // User::create_user_tables(&pool).await;

            // build our application with some routes
            let app = Router::new()
                // Server side render the application, serve static assets, and register server functions
                .serve_dioxus_application(ServeConfig::new().unwrap(), app)
                .layer(session_layer);

            // run it
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

            axum::serve(listener, app.into_make_service())
                .await
                .unwrap();
        });
}