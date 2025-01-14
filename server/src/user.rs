use anyhow::{Context, Result};
use async_trait::async_trait;
use axum_session_auth::{Authentication, HasPermission};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashSet;

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
        Ok(sqlx::query_as(
            "SELECT id, username, email, is_active, full_name, avatar_url FROM users WHERE id=$1",
        )
        .bind(id)
        .fetch_one(pool)
        .await?)
    }
}

#[derive(sqlx::FromRow, Clone)]
pub struct SqlPermissionTokens {
    pub token: String,
}

impl SqlPermissionTokens {
    pub async fn read(user_id: i32, pool: &PgPool) -> Result<Vec<SqlPermissionTokens>> {
        Ok(
            sqlx::query_as("SELECT token FROM user_permissions WHERE user_id=$1")
                .bind(user_id)
                .fetch_all(pool)
                .await?,
        )
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

/// A composite user with attached properties
impl User {
    pub async fn read(id: i32, pool: &PgPool) -> Result<User> {
        let sql_user = SqlUser::read(id, pool).await?;
        //lets just get all the tokens the user can use, we will only use the full permissions if modifying them.
        let sql_user_perms = SqlPermissionTokens::read(id, pool).await?;
        Ok(sql_user.into_user(Some(sql_user_perms)))
    }
}

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

#[async_trait]
impl HasPermission<PgPool> for User {
    async fn has(&self, perm: &str, _pool: &Option<&PgPool>) -> bool {
        self.permissions.contains(perm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::settings::Settings;
    use tokio::runtime::Runtime;

    #[test]
    fn test_user_retrieval() {
        // Create the runtime
        let rt = Runtime::new().unwrap();

        rt.block_on(async {
            let settings = Settings::new().unwrap_or_default();
            let url = settings.database.url();
            // Create a connection pool
            let pool = PgPool::connect(&url).await.expect("DB connection error");
            // Make sure migrations were run
            let user = User::read(1, &pool).await.expect("Cannot pull user");
            println!("User = {user:?}");
        });
    }
}
