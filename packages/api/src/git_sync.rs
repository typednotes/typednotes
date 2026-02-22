//! Git synchronisation helpers (server-only).
//!
//! Each user gets a bare working directory under `/data/repos/{user_id}/`.
//! All operations use libgit2 via the `git2` crate with in-memory SSH credentials.

use git2::{
    Cred, FetchOptions, IndexAddOption, PushOptions, RemoteCallbacks, Repository, Signature,
};
use std::fs;
use std::path::{Path, PathBuf};

/// Root directory for all per-user repositories.
fn repos_root() -> PathBuf {
    std::env::var("GIT_REPOS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/data/repos"))
}

/// Return the on-disk path for a user's local clone.
pub fn repo_path(user_id: &uuid::Uuid) -> PathBuf {
    repos_root().join(user_id.to_string())
}

/// Build `RemoteCallbacks` that authenticate via an in-memory SSH private key.
fn make_callbacks(ssh_key_pem: &str) -> RemoteCallbacks<'_> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_url, username_from_url, _allowed| {
        let user = username_from_url.unwrap_or("git");
        Cred::ssh_key_from_memory(user, None, ssh_key_pem, None)
    });
    callbacks
}

/// Open an existing clone **or** clone the remote for the first time.
/// Returns the opened `Repository` handle.
pub fn ensure_repo(
    user_id: &uuid::Uuid,
    remote_url: &str,
    ssh_key_pem: &str,
) -> Result<Repository, String> {
    let path = repo_path(user_id);

    if path.join(".git").exists() || path.join("HEAD").exists() {
        // Already cloned — just open.
        Repository::open(&path).map_err(|e| format!("Failed to open repo: {e}"))
    } else {
        // Fresh clone.
        fs::create_dir_all(&path).map_err(|e| format!("mkdir: {e}"))?;

        let callbacks = make_callbacks(ssh_key_pem);
        let mut fo = FetchOptions::new();
        fo.remote_callbacks(callbacks);

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fo);

        let repo = builder
            .clone(remote_url, &path)
            .map_err(|e| format!("Clone failed: {e}"))?;

        Ok(repo)
    }
}

/// Fetch from origin and hard-reset to the remote tracking branch.
pub fn pull(repo: &Repository, ssh_key_pem: &str) -> Result<(), String> {
    // Fetch
    let callbacks = make_callbacks(ssh_key_pem);
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(callbacks);

    let mut remote = repo
        .find_remote("origin")
        .map_err(|e| format!("No origin remote: {e}"))?;

    remote
        .fetch(&[] as &[&str], Some(&mut fo), None)
        .map_err(|e| format!("Fetch failed: {e}"))?;

    // Determine the default branch
    let head = repo.head().map_err(|e| format!("No HEAD: {e}"))?;
    let branch_name = head
        .shorthand()
        .unwrap_or("main")
        .to_string();

    let remote_ref = format!("refs/remotes/origin/{branch_name}");
    let remote_oid = repo
        .refname_to_id(&remote_ref)
        .map_err(|e| format!("Cannot resolve {remote_ref}: {e}"))?;

    let remote_commit = repo
        .find_commit(remote_oid)
        .map_err(|e| format!("Cannot find remote commit: {e}"))?;

    // Reset working tree to remote
    repo.reset(
        remote_commit.as_object(),
        git2::ResetType::Hard,
        None,
    )
    .map_err(|e| format!("Reset failed: {e}"))?;

    Ok(())
}

/// Write `content` to `path` (relative to worktree), stage, commit, and push.
pub fn sync_file(
    repo: &Repository,
    path: &str,
    content: &str,
    message: &str,
    ssh_key_pem: &str,
) -> Result<(), String> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| "Bare repository".to_string())?;
    let file_path = workdir.join(path);

    // Ensure parent directories exist
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }

    fs::write(&file_path, content).map_err(|e| format!("write: {e}"))?;

    commit_and_push(repo, message, ssh_key_pem)
}

/// Remove a file from the worktree and index, commit, and push.
pub fn delete_file(
    repo: &Repository,
    path: &str,
    message: &str,
    ssh_key_pem: &str,
) -> Result<(), String> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| "Bare repository".to_string())?;
    let file_path = workdir.join(path);

    if file_path.exists() {
        fs::remove_file(&file_path).map_err(|e| format!("rm: {e}"))?;
    }

    commit_and_push(repo, message, ssh_key_pem)
}

/// Stage all changes, commit, and push to origin.
fn commit_and_push(repo: &Repository, message: &str, ssh_key_pem: &str) -> Result<(), String> {
    let mut index = repo.index().map_err(|e| format!("index: {e}"))?;
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .map_err(|e| format!("add_all: {e}"))?;
    // Also pick up deletions
    index
        .update_all(["*"].iter(), None)
        .map_err(|e| format!("update_all: {e}"))?;
    index.write().map_err(|e| format!("index write: {e}"))?;

    let tree_oid = index
        .write_tree()
        .map_err(|e| format!("write_tree: {e}"))?;
    let tree = repo
        .find_tree(tree_oid)
        .map_err(|e| format!("find_tree: {e}"))?;

    let sig = Signature::now("TypedNotes", "sync@typednotes.org")
        .map_err(|e| format!("sig: {e}"))?;

    let parent = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok());

    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();

    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .map_err(|e| format!("commit: {e}"))?;

    // Push
    let callbacks = make_callbacks(ssh_key_pem);
    let mut po = PushOptions::new();
    po.remote_callbacks(callbacks);

    let head = repo.head().map_err(|e| format!("HEAD: {e}"))?;
    let refspec = head
        .name()
        .ok_or_else(|| "Cannot determine HEAD refspec".to_string())?;

    let mut remote = repo
        .find_remote("origin")
        .map_err(|e| format!("No origin: {e}"))?;

    remote
        .push(&[refspec], Some(&mut po))
        .map_err(|e| format!("push: {e}"))?;

    Ok(())
}

/// Walk the worktree and return `(relative_path, content)` for all note files.
pub fn list_files(repo: &Repository) -> Result<Vec<(String, String)>, String> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| "Bare repository".to_string())?;

    let mut results = Vec::new();
    walk_dir(workdir, workdir, &mut results)?;
    Ok(results)
}

fn walk_dir(base: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("readdir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let path = entry.path();

        // Skip .git directory
        if path
            .file_name()
            .map(|n| n == ".git")
            .unwrap_or(false)
        {
            continue;
        }

        if path.is_dir() {
            walk_dir(base, &path, out)?;
        } else if is_note_file(&path) {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| format!("strip_prefix: {e}"))?
                .to_string_lossy()
                .to_string();
            let content = fs::read_to_string(&path).unwrap_or_default();
            out.push((rel, content));
        }
    }
    Ok(())
}

fn is_note_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md" | "txt" | "toml")
    )
}
