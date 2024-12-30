use dioxus::prelude::*;
use axum::routing::*;
use axum_session::{SessionConfig, SessionStore};
use axum_session_auth::{AuthConfig, SessionSqlitePool};



pub fn launch(app: fn() -> Element) {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            let pool = connect_to_database().await;
            //This Defaults as normal Cookies.
            //To enable Private cookies for integrity, and authenticity please check the next Example.
            let session_config = SessionConfig::default().with_table_name("test_table");
            let auth_config = AuthConfig::<i64>::default().with_anonymous_user_id(Some(1));
            let session_store = SessionStore::<SessionSqlitePool>::new(
                Some(pool.clone().into()),
                session_config,
            )
            .await
            .unwrap();

            User::create_user_tables(&pool).await;

            // build our application with some routes
            let app = Router::new()
                // Server side render the application, serve static assets, and register server functions
                .serve_dioxus_application(ServeConfig::new().unwrap(), app)
                .layer(
                    axum_session_auth::AuthSessionLayer::<
                        crate::auth::User,
                        i64,
                        axum_session_auth::SessionSqlitePool,
                        sqlx::SqlitePool,
                    >::new(Some(pool))
                    .with_config(auth_config),
                )
                .layer(axum_session::SessionLayer::new(session_store));

            // run it
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

            axum::serve(listener, app.into_make_service())
                .await
                .unwrap();
        });
}