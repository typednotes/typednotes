use std::{
    env,
    sync::Arc,
    collections::HashSet,
};
use anyhow::{Context, Result};
use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_session_auth::{AuthSession, AuthSessionLayer, Authentication, AuthConfig, HasPermission};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use async_trait::async_trait;

// https://github.com/DioxusLabs/dioxus/blob/v0.6/examples/fullstack-auth/src/auth.rs

/// Database backed user
#[derive(sqlx::FromRow, Clone)]
pub struct SqlUser {
    pub id: i32,
    pub username: String,
    pub email: String,
    pub is_active: bool,
    pub full_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl SqlUser {
    /// Build a user from a sql user
    pub fn into_user(self, sql_user_perms: Option<Vec<SqlPermissionTokens>>) -> User {
        User {
            id: self.id,
            username: self.username,
            email: self.email,
            is_active: self.is_active,
            full_name: self.full_name,
            avatar_url: self.avatar_url,
            permissions: if let Some(user_perms) = sql_user_perms {
                user_perms
                    .into_iter()
                    .map(|x| x.token)
                    .collect::<HashSet<String>>()
            } else {
                HashSet::<String>::new()
            },
        }
    }

    pub async fn read(id: i32, pool: &PgPool) -> Result<SqlUser> {
        Ok(sqlx::query_as("SELECT id, username, email, is_active, full_name, avatar_url FROM users WHERE id = %1").bind(id).fetch_one(pool).await?)
    }
}


#[derive(sqlx::FromRow, Clone)]
pub struct SqlPermissionTokens {
    pub token: String,
}

impl SqlPermissionTokens {
    pub async fn read(user_id: i32, pool: &PgPool) -> Result<Vec<SqlPermissionTokens>> {
        Ok(sqlx::query_as("SELECT token FROM user_permissions WHERE user_id = %1").bind(user_id).fetch_all(pool).await?)
    }
}

/// User with permissions etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub email: String,
    pub is_active: bool,
    pub full_name: Option<String>,
    pub avatar_url: Option<String>,
    pub permissions: HashSet<String>,
}

impl User {
    pub async fn read(id: i32, pool: &PgPool) -> Result<User> {
        let sql_user = SqlUser::read(id, pool).await?;
        //lets just get all the tokens the user can use, we will only use the full permissions if modifying them.
        let sql_user_perms = SqlPermissionTokens::read(id, pool).await?;
        Ok(sql_user.into_user(Some(sql_user_perms)))
    }
}

// #[async_trait]
// impl Authentication<User, i64, SqlitePool> for User {
//     async fn load_user(userid: i64, pool: Option<&SqlitePool>) -> Result<User, anyhow::Error> {
//         let pool = pool.unwrap();

//         User::get_user(userid, pool)
//             .await
//             .ok_or_else(|| anyhow::anyhow!("Could not load user"))
//     }

//     fn is_authenticated(&self) -> bool {
//         !self.anonymous
//     }

//     fn is_active(&self) -> bool {
//         !self.anonymous
//     }

//     fn is_anonymous(&self) -> bool {
//         self.anonymous
//     }
// }

// #[async_trait]
// impl HasPermission<SqlitePool> for User {
//     async fn has(&self, perm: &str, _pool: &Option<&SqlitePool>) -> bool {
//         self.permissions.contains(perm)
//     }
// }

#[async_trait]
impl Authentication<User, i32, PgPool> for User {
    // This is run when the user has logged in and has not yet been Cached in the system.
    // Once ran it will load and cache the user.
    async fn load_user(id: i32, pool: Option<&PgPool>) -> Result<User> {
        let pool = pool.context("No pool")?;
        let user = User::read(id, pool).await?;
        Ok(user)
    }

    // This function is used internally to determine if they are logged in or not.
    fn is_authenticated(&self) -> bool {
        self.is_active
    }

    fn is_active(&self) -> bool {
        self.is_active
    }

    fn is_anonymous(&self) -> bool {
        !self.is_active
    }
}

// // Initialize router with authentication routes
// async fn init_router(state: AppState) -> Router {
//     Router::new()
//         .route("/auth/github/login", get(github_login))
//         .route("/auth/github/callback", get(github_callback))
//         .route("/logout", post(logout))
//         .with_state(state)
// }

// // GitHub login handler
// async fn github_login(
//     State(state): State<AppState>,
//     session: Session,
// ) -> impl IntoResponse {
//     let (auth_url, csrf_token) = state
//         .oauth_client
//         .authorize_url(CsrfToken::new_random)
//         .url();

//     session.insert("csrf_token", csrf_token.secret()).await.unwrap();
//     Redirect::to(auth_url.as_str())
// }

// // GitHub callback handler
// async fn github_callback(
//     State(state): State<AppState>,
//     session: Session,
//     Query(params): Query<HashMap<String, String>>,
// ) -> impl IntoResponse {
//     let code = params.get("code").expect("No code provided");
//     let state_param = params.get("state").expect("No state provided");
    
//     // Verify CSRF token
//     let stored_state: String = session
//         .get("csrf_token")
//         .await
//         .unwrap()
//         .expect("No CSRF token in session");
    
//     if state_param != &stored_state {
//         return Redirect::to("/auth/error?error=csrf_mismatch");
//     }

//     // Exchange the code for an access token
//     let token = state
//         .oauth_client
//         .exchange_code(AuthorizationCode::new(code.to_string()))
//         .request(oauth2::reqwest::async_http_client)
//         .await
//         .expect("Failed to exchange code for token");

//     // Get GitHub user data
//     let client = reqwest::Client::new();
//     let github_user: Value = client
//         .get("https://api.github.com/user")
//         .bearer_auth(token.access_token().secret())
//         .header("User-Agent", "rust-web-app")
//         .send()
//         .await
//         .unwrap()
//         .json()
//         .await
//         .unwrap();

//     // Create or update user in database
//     let user = upsert_user(&state.pool, &github_user).await?;
    
//     // Set user session
//     session.insert("user_id", user.id).await?;
    
//     Redirect::to("/dashboard")
// }

// // Upsert user in database
// async fn upsert_user(pool: &Pool<Postgres>, github_user: &Value) -> Result<User, sqlx::Error> {
//     let github_id = github_user["id"].as_i64().unwrap();
//     let username = github_user["login"].as_str().unwrap();
//     let email = github_user["email"].as_str().map(|s| s.to_string());
//     let avatar_url = github_user["avatar_url"].as_str().map(|s| s.to_string());

//     sqlx::query_as!(
//         User,
//         r#"
//         INSERT INTO users (github_id, username, email, avatar_url)
//         VALUES ($1, $2, $3, $4)
//         ON CONFLICT (github_id) DO UPDATE
//         SET username = $2, email = $3, avatar_url = $4
//         RETURNING id, github_id, username, email, avatar_url
//         "#,
//         github_id,
//         username,
//         email,
//         avatar_url,
//     )
//     .fetch_one(pool)
//     .await
// }

// // Logout handler
// async fn logout(session: Session) -> impl IntoResponse {
//     session.destroy();
//     Redirect::to("/")
// }

// // Main function
// #[tokio::main]
// async fn main() {
//     // Load environment variables
//     dotenv::dotenv().ok();
    
//     // Initialize database connection
//     let pool = init_db().await;
    
//     // Run database migrations
//     sqlx::migrate!("./migrations")
//         .run(&pool)
//         .await
//         .expect("Failed to run migrations");

//     // Initialize OAuth client
//     let oauth_client = Arc::new(init_oauth_client());
    
//     // Create app state
//     let state = AppState {
//         pool,
//         oauth_client,
//     };

//     // Build the router
//     let app = init_router(state).await;
    
//     // Add session layer
//     let session_store = tower_sessions_sqlx_store::PostgresStore::new(pool);
//     let session_layer = SessionManager::new(session_store)
//         .with_secure(false)  // Set to true in production
//         .with_name("session");
    
//     let app = app.layer(session_layer);

//     // Start the server
//     let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
//     println!("Server running on http://{}", addr);
    
//     axum::Server::bind(&addr)
//         .serve(app.into_make_service())
//         .await
//         .unwrap();
// }