use std::any::Any;
use dioxus::prelude::*;
use time::Duration;
use tower_sessions::{Expiry, Session, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;
use super::database::connection_pool;

/// Lanch a server with a session store for authentication
/// see https://crates.io/crates/dioxus-fullstack
pub fn launch(app: fn() -> Element) {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            // Get the address the server should run on. If the CLI is running, the CLI proxies fullstack into the main address
            // and we use the generated address the CLI gives us
            let address = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));

            // Set up the axum router
            let router = axum::Router::new()
                // You can add a dioxus application to the router with the `serve_dioxus_application` method
                // This will add a fallback route to the router that will serve your component and server functions
                .serve_dioxus_application(ServeConfigBuilder::default(), app);

            // Finally, we can launch the server
            let router = router.into_make_service();
            let listener = tokio::net::TcpListener::bind(address).await.unwrap();
            axum::serve(listener, router).await.unwrap();
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