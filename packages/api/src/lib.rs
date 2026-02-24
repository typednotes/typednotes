//! # API crate — shared fullstack server functions for TypedNotes
//!
//! This crate is the backbone of the TypedNotes fullstack architecture. It defines every
//! Dioxus server function that the web, desktop, and mobile frontends call, along with
//! the supporting modules they depend on.
//!
//! ## Modules
//!
//! | Module | Feature gate | Purpose |
//! |--------|-------------|---------|
//! | [`auth`] | — | OAuth (GitHub, Google) and local password authentication, session management, password hashing |
//! | [`crypto`] | `server` | AES-GCM encryption/decryption of SSH private keys, public key extraction |
//! | [`db`] | — | PostgreSQL connection pool (lazy `OnceCell` singleton) and migrations |
//! | [`git_transport`] | `server` | Low-level Git fetch/push over SSH using an in-memory object store |
//! | [`models`] | — | Database models (`User`) and their client-safe projections (`UserInfo`) |
//!
//! ## Server functions exposed here
//!
//! Every public `async fn` in this file is a Dioxus server function, annotated with
//! `#[get(...)]` or `#[post(...)]` and compiled twice: once with full server logic
//! (behind `#[cfg(feature = "server")]`) and once as a thin client stub that simply
//! forwards the call over HTTP.
//!
//! - **Authentication**: `get_current_user`, `get_login_url`, `logout`, `register`, `login_password`
//! - **Git credentials**: `save_git_credentials`, `get_git_credentials`
//! - **Git sync**: `sync_note`, `delete_note_remote`, `pull_notes`

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

pub mod auth;
#[cfg(feature = "server")]
pub mod crypto;
pub mod db;
#[cfg(feature = "server")]
pub mod git_transport;
pub mod models;

pub use models::UserInfo;
pub use store::{NamespaceInfo, TypedNoteInfo};

pub use store::TypedNotesConfig;

/// Git credentials info safe to send to the client (never includes private key).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitCredentialsInfo {
    pub git_remote_url: Option<String>,
    pub ssh_public_key: Option<String>,
    pub git_branch: Option<String>,
}

/// Get the current authenticated user from the session.
#[cfg(feature = "server")]
#[get("/api/auth/me", session: tower_sessions::Session)]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    use crate::db::get_pool;
    use crate::models::User;

    let user_id: Option<String> = session
        .get(auth::SESSION_USER_ID_KEY)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some(user_id) = user_id else {
        return Ok(None);
    };

    let pool = get_pool()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let user_uuid = uuid::Uuid::parse_str(&user_id)
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_uuid)
        .fetch_optional(pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(user.map(|u| u.to_info()))
}

#[cfg(not(feature = "server"))]
#[get("/api/auth/me")]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    Ok(None)
}

/// Get the OAuth login URL for a provider.
#[cfg(feature = "server")]
#[get("/api/auth/login/:provider")]
pub async fn get_login_url(provider: String) -> Result<String, ServerFnError> {
    match provider.as_str() {
        "github" => {
            let oauth = auth::GitHubOAuth::new()
                .map_err(|e| ServerFnError::new(e))?;
            let (url, _, _) = oauth
                .generate_auth_url()
                .await
                .map_err(|e| ServerFnError::new(e))?;
            Ok(url)
        }
        "google" => {
            let oauth = auth::GoogleOAuth::new()
                .map_err(|e| ServerFnError::new(e))?;
            let (url, _, _) = oauth
                .generate_auth_url()
                .await
                .map_err(|e| ServerFnError::new(e))?;
            Ok(url)
        }
        _ => Err(ServerFnError::new(format!("Unknown provider: {}", provider))),
    }
}

#[cfg(not(feature = "server"))]
#[get("/api/auth/login/:provider")]
pub async fn get_login_url(provider: String) -> Result<String, ServerFnError> {
    Err(ServerFnError::new("Server only"))
}

/// Log out the current user by clearing the session.
#[cfg(feature = "server")]
#[post("/api/auth/logout", session: tower_sessions::Session)]
pub async fn logout() -> Result<(), ServerFnError> {
    session
        .flush()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(())
}

#[cfg(not(feature = "server"))]
#[post("/api/auth/logout")]
pub async fn logout() -> Result<(), ServerFnError> {
    Ok(())
}

/// Register a new user with email and password.
#[cfg(feature = "server")]
#[post("/api/auth/register", session: tower_sessions::Session)]
pub async fn register(
    email: String,
    password: String,
    name: String,
) -> Result<UserInfo, ServerFnError> {
    use crate::db::get_pool;

    let email = email.trim().to_lowercase();
    let name = name.trim().to_string();

    if email.is_empty() || !email.contains('@') {
        return Err(ServerFnError::new("Invalid email address"));
    }
    if password.len() < 8 {
        return Err(ServerFnError::new(
            "Password must be at least 8 characters",
        ));
    }
    if name.is_empty() {
        return Err(ServerFnError::new("Name is required"));
    }

    let pool = get_pool()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    // Check if user already exists
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 as n FROM users WHERE provider = 'local' AND provider_id = $1",
    )
    .bind(&email)
    .fetch_optional(pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    if existing.is_some() {
        return Err(ServerFnError::new("An account with this email already exists"));
    }

    let password_hash = auth::hash_password(&password)
        .map_err(|e| ServerFnError::new(e))?;

    let user: models::User = sqlx::query_as(
        "INSERT INTO users (email, name, provider, provider_id, password_hash) VALUES ($1, $2, 'local', $1, $3) RETURNING *",
    )
    .bind(&email)
    .bind(&name)
    .bind(&password_hash)
    .fetch_one(pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    session
        .insert(auth::SESSION_USER_ID_KEY, user.id.to_string())
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(user.to_info())
}

#[cfg(not(feature = "server"))]
#[post("/api/auth/register")]
pub async fn register(
    email: String,
    password: String,
    name: String,
) -> Result<UserInfo, ServerFnError> {
    Err(ServerFnError::new("Server only"))
}

/// Log in with email and password.
#[cfg(feature = "server")]
#[post("/api/auth/login-password", session: tower_sessions::Session)]
pub async fn login_password(email: String, password: String) -> Result<UserInfo, ServerFnError> {
    use crate::db::get_pool;

    let email = email.trim().to_lowercase();

    let pool = get_pool()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let user: Option<models::User> = sqlx::query_as(
        "SELECT * FROM users WHERE provider = 'local' AND provider_id = $1",
    )
    .bind(&email)
    .fetch_optional(pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some(user) = user else {
        return Err(ServerFnError::new("Invalid email or password"));
    };

    let Some(ref hash) = user.password_hash else {
        return Err(ServerFnError::new("Invalid email or password"));
    };

    let valid = auth::verify_password(&password, hash)
        .map_err(|e| ServerFnError::new(e))?;

    if !valid {
        return Err(ServerFnError::new("Invalid email or password"));
    }

    session
        .insert(auth::SESSION_USER_ID_KEY, user.id.to_string())
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(user.to_info())
}

#[cfg(not(feature = "server"))]
#[post("/api/auth/login-password")]
pub async fn login_password(email: String, password: String) -> Result<UserInfo, ServerFnError> {
    Err(ServerFnError::new("Server only"))
}

/// Save git credentials (remote URL, optional SSH key, optional branch).
#[cfg(feature = "server")]
#[post("/api/git/credentials", session: tower_sessions::Session)]
pub async fn save_git_credentials(
    git_remote_url: String,
    ssh_private_key: Option<String>,
    git_branch: Option<String>,
) -> Result<GitCredentialsInfo, ServerFnError> {
    use crate::db::get_pool;

    let user_id: Option<String> = session
        .get(auth::SESSION_USER_ID_KEY)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some(user_id) = user_id else {
        return Err(ServerFnError::new("Not authenticated"));
    };

    let user_uuid =
        uuid::Uuid::parse_str(&user_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    let pool = get_pool()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let git_remote_url = if git_remote_url.trim().is_empty() {
        None
    } else {
        Some(git_remote_url.trim().to_string())
    };

    // If SSH key provided, encrypt it and extract public key
    let (encrypted_key, nonce, public_key) = if let Some(ref key_pem) = ssh_private_key {
        if key_pem.trim().is_empty() {
            (None, None, None)
        } else {
            let pub_key = crypto::extract_public_key(key_pem)
                .map_err(|e| ServerFnError::new(e))?;
            let (enc, n) = crypto::encrypt_ssh_key(key_pem.as_bytes())
                .map_err(|e| ServerFnError::new(e))?;
            (Some(enc), Some(n), Some(pub_key))
        }
    } else {
        (None, None, None)
    };

    let branch = git_branch
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());

    if encrypted_key.is_some() {
        // Upsert with new SSH key
        sqlx::query(
            "INSERT INTO user_git_config (user_id, git_remote_url, ssh_private_key_enc, ssh_public_key, encryption_nonce, git_branch)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (user_id) DO UPDATE SET
                git_remote_url = $2,
                ssh_private_key_enc = $3,
                ssh_public_key = $4,
                encryption_nonce = $5,
                git_branch = $6,
                updated_at = NOW()",
        )
        .bind(user_uuid)
        .bind(&git_remote_url)
        .bind(&encrypted_key)
        .bind(&public_key)
        .bind(&nonce)
        .bind(&branch)
        .execute(pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    } else {
        // Upsert URL + branch only, preserve existing SSH key
        sqlx::query(
            "INSERT INTO user_git_config (user_id, git_remote_url, git_branch)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id) DO UPDATE SET
                git_remote_url = $2,
                git_branch = $3,
                updated_at = NOW()",
        )
        .bind(user_uuid)
        .bind(&git_remote_url)
        .bind(&branch)
        .execute(pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    }

    // Fetch back the saved state
    let row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT git_remote_url, ssh_public_key, git_branch FROM user_git_config WHERE user_id = $1",
    )
    .bind(user_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(match row {
        Some((url, pub_key, branch)) => GitCredentialsInfo {
            git_remote_url: url,
            ssh_public_key: pub_key,
            git_branch: Some(branch),
        },
        None => GitCredentialsInfo {
            git_remote_url: None,
            ssh_public_key: None,
            git_branch: Some("main".to_string()),
        },
    })
}

#[cfg(not(feature = "server"))]
#[post("/api/git/credentials")]
pub async fn save_git_credentials(
    git_remote_url: String,
    ssh_private_key: Option<String>,
    git_branch: Option<String>,
) -> Result<GitCredentialsInfo, ServerFnError> {
    Err(ServerFnError::new("Server only"))
}

/// Get git credentials for the current user (URL + public key only).
#[cfg(feature = "server")]
#[get("/api/git/credentials", session: tower_sessions::Session)]
pub async fn get_git_credentials() -> Result<Option<GitCredentialsInfo>, ServerFnError> {
    use crate::db::get_pool;

    let user_id: Option<String> = session
        .get(auth::SESSION_USER_ID_KEY)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some(user_id) = user_id else {
        return Err(ServerFnError::new("Not authenticated"));
    };

    let user_uuid =
        uuid::Uuid::parse_str(&user_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    let pool = get_pool()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT git_remote_url, ssh_public_key, git_branch FROM user_git_config WHERE user_id = $1",
    )
    .bind(user_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(row.map(|(url, pub_key, branch)| GitCredentialsInfo {
        git_remote_url: url,
        ssh_public_key: pub_key,
        git_branch: Some(branch),
    }))
}

#[cfg(not(feature = "server"))]
#[get("/api/git/credentials")]
pub async fn get_git_credentials() -> Result<Option<GitCredentialsInfo>, ServerFnError> {
    Err(ServerFnError::new("Server only"))
}

/// A file retrieved from the remote git repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFile {
    pub path: String,
    pub content: String,
}

/// Helper: get user_id, remote URL, decrypted SSH key, and branch from the session + DB.
#[cfg(feature = "server")]
async fn get_user_git_context(
    session: &tower_sessions::Session,
) -> Result<(uuid::Uuid, String, String, String), ServerFnError> {
    use crate::db::get_pool;

    let user_id: Option<String> = session
        .get(auth::SESSION_USER_ID_KEY)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some(user_id) = user_id else {
        return Err(ServerFnError::new("Not authenticated"));
    };

    let user_uuid =
        uuid::Uuid::parse_str(&user_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    let pool = get_pool()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let row: Option<(Option<String>, Option<Vec<u8>>, Option<Vec<u8>>, String)> = sqlx::query_as(
        "SELECT git_remote_url, ssh_private_key_enc, encryption_nonce, git_branch FROM user_git_config WHERE user_id = $1",
    )
    .bind(user_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    let Some((Some(remote_url), Some(enc_key), Some(nonce), branch)) = row else {
        return Err(ServerFnError::new(
            "Git sync not configured: set remote URL and SSH key in Settings",
        ));
    };

    let key_bytes =
        crypto::decrypt_ssh_key(&enc_key, &nonce).map_err(|e| ServerFnError::new(e))?;
    let ssh_key_pem =
        String::from_utf8(key_bytes).map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok((user_uuid, remote_url, ssh_key_pem, branch))
}

/// Sync a single note to the git remote: fetch, write note in memory, push.
#[cfg(feature = "server")]
#[post("/api/git/sync-note", session: tower_sessions::Session)]
pub async fn sync_note(
    path: String,
    content: String,
    note_type: String,
) -> Result<(), ServerFnError> {
    let (_user_id, remote_url, ssh_key_pem, branch) = get_user_git_context(&session).await?;

    let mem = store::MemoryStore::new();
    let repo = store::Repository::new(mem.clone());

    // Fetch current state from remote (blocking I/O)
    let mem2 = mem.clone();
    let url = remote_url.clone();
    let key = ssh_key_pem.clone();
    let branch2 = branch.clone();
    tokio::task::spawn_blocking(move || git_transport::fetch(&mem2, &url, &key, Some(&branch2)))
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .map_err(|e| ServerFnError::new(e))?;

    // Snapshot SHAs before modification
    let pre_shas: std::collections::HashSet<String> =
        mem.all_object_shas().into_iter().collect();

    // Write note in memory (fast, no I/O)
    repo.write_note(&path, &content, &note_type).await;

    // Determine new objects to push
    let new_shas: Vec<String> = mem
        .all_object_shas()
        .into_iter()
        .filter(|s| !pre_shas.contains(s))
        .collect();

    // Push to remote (blocking I/O)
    let mem2 = mem.clone();
    tokio::task::spawn_blocking(move || {
        git_transport::push(&mem2, &remote_url, &ssh_key_pem, &branch, &new_shas)
    })
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?
    .map_err(|e| ServerFnError::new(e))?;

    Ok(())
}

#[cfg(not(feature = "server"))]
#[post("/api/git/sync-note")]
pub async fn sync_note(
    path: String,
    content: String,
    note_type: String,
) -> Result<(), ServerFnError> {
    Err(ServerFnError::new("Server only"))
}

/// Delete a note from the git remote: fetch, delete in memory, push.
#[cfg(feature = "server")]
#[post("/api/git/delete-note", session: tower_sessions::Session)]
pub async fn delete_note_remote(path: String) -> Result<(), ServerFnError> {
    let (_user_id, remote_url, ssh_key_pem, branch) = get_user_git_context(&session).await?;

    let mem = store::MemoryStore::new();
    let repo = store::Repository::new(mem.clone());

    // Fetch
    let mem2 = mem.clone();
    let url = remote_url.clone();
    let key = ssh_key_pem.clone();
    let branch2 = branch.clone();
    tokio::task::spawn_blocking(move || git_transport::fetch(&mem2, &url, &key, Some(&branch2)))
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .map_err(|e| ServerFnError::new(e))?;

    let pre_shas: std::collections::HashSet<String> =
        mem.all_object_shas().into_iter().collect();

    // Delete note in memory
    repo.delete_note(&path).await;

    let new_shas: Vec<String> = mem
        .all_object_shas()
        .into_iter()
        .filter(|s| !pre_shas.contains(s))
        .collect();

    // Push
    let mem2 = mem.clone();
    tokio::task::spawn_blocking(move || {
        git_transport::push(&mem2, &remote_url, &ssh_key_pem, &branch, &new_shas)
    })
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?
    .map_err(|e| ServerFnError::new(e))?;

    Ok(())
}

#[cfg(not(feature = "server"))]
#[post("/api/git/delete-note")]
pub async fn delete_note_remote(path: String) -> Result<(), ServerFnError> {
    Err(ServerFnError::new("Server only"))
}

/// Pull all notes from the git remote.
#[cfg(feature = "server")]
#[get("/api/git/pull", session: tower_sessions::Session)]
pub async fn pull_notes() -> Result<Vec<RemoteFile>, ServerFnError> {
    let (_user_id, remote_url, ssh_key_pem, branch) = get_user_git_context(&session).await?;

    let mem = store::MemoryStore::new();
    let repo = store::Repository::new(mem.clone());

    // Fetch
    tokio::task::spawn_blocking(move || {
        git_transport::fetch(&mem, &remote_url, &ssh_key_pem, Some(&branch))
    })
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?
    .map_err(|e| ServerFnError::new(e))?;

    // List notes from in-memory repo
    let notes = repo.list_notes().await;

    Ok(notes
        .into_iter()
        .map(|n| RemoteFile {
            path: n.path,
            content: n.note,
        })
        .collect())
}

#[cfg(not(feature = "server"))]
#[get("/api/git/pull")]
pub async fn pull_notes() -> Result<Vec<RemoteFile>, ServerFnError> {
    Err(ServerFnError::new("Server only"))
}
