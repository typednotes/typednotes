//! # In-memory object store
//!
//! [`MemoryStore`] is an [`ObjectStore`] implementation that keeps every Git object
//! and ref in process memory, backed by `Arc<Mutex<HashMap>>` maps. It is used in
//! two contexts:
//!
//! - **Server-side Git sync** — each fetch/push cycle in [`api::git_transport`] creates
//!   a fresh `MemoryStore`, populates it via `fetch`, lets [`crate::Repository`] read
//!   or mutate the tree, and then `push`es the result. The store is discarded once the
//!   request completes — there is no persistent server-side state beyond the remote.
//!
//! - **Tests** — the `#[cfg(test)]` section at the bottom of this file exercises the
//!   full `Repository` API (write, read, delete, namespaces, config, scoped listing)
//!   against a `MemoryStore`, validating the entire storage layer without I/O.
//!
//! ## Dual API surface
//!
//! Because the Git transport code runs inside `tokio::task::spawn_blocking`, it cannot
//! call `async` methods. `MemoryStore` therefore exposes **both**:
//!
//! - The async [`ObjectStore`] trait (`get`, `put`, `get_ref`, `set_ref`) — used by
//!   `Repository` in normal async code.
//! - Synchronous equivalents (`get_sync`, `put_sync`, `get_ref_sync`, `set_ref_sync`)
//!   — used by the blocking Git transport layer.
//!
//! Both surfaces access the same underlying `Arc<Mutex<…>>` maps, so data written via
//! one is immediately visible to the other.
//!
//! ## `all_object_shas`
//!
//! A convenience method that returns the hex SHA keys of every stored object. Used by
//! the server functions in [`api`] to compute the set of newly created objects (by
//! diffing snapshots before and after a `Repository` write) so that only those objects
//! are included in the push packfile.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::objects::Sha;
use crate::repo::ObjectStore;

/// In-memory ObjectStore for testing and desktop fallback.
#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    refs: Arc<Mutex<HashMap<String, Sha>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronous get — for use in blocking contexts (e.g. git transport).
    pub fn get_sync(&self, sha: &Sha) -> Option<Vec<u8>> {
        self.objects.lock().unwrap().get(&sha.to_hex()).cloned()
    }

    /// Synchronous put — for use in blocking contexts.
    pub fn put_sync(&self, sha: &Sha, data: Vec<u8>) {
        self.objects.lock().unwrap().insert(sha.to_hex(), data);
    }

    /// Synchronous get_ref — for use in blocking contexts.
    pub fn get_ref_sync(&self, name: &str) -> Option<Sha> {
        self.refs.lock().unwrap().get(name).cloned()
    }

    /// Synchronous set_ref — for use in blocking contexts.
    pub fn set_ref_sync(&self, name: &str, sha: &Sha) {
        self.refs
            .lock()
            .unwrap()
            .insert(name.to_string(), sha.clone());
    }

    /// Return the hex SHA strings of all stored objects.
    pub fn all_object_shas(&self) -> Vec<String> {
        self.objects.lock().unwrap().keys().cloned().collect()
    }
}

impl ObjectStore for MemoryStore {
    async fn get(&self, sha: &Sha) -> Option<Vec<u8>> {
        self.objects.lock().unwrap().get(&sha.to_hex()).cloned()
    }

    async fn put(&self, sha: &Sha, data: Vec<u8>) {
        self.objects.lock().unwrap().insert(sha.to_hex(), data);
    }

    async fn get_ref(&self, name: &str) -> Option<Sha> {
        self.refs.lock().unwrap().get(name).cloned()
    }

    async fn set_ref(&self, name: &str, sha: &Sha) {
        self.refs.lock().unwrap().insert(name.to_string(), sha.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repository;

    #[tokio::test]
    async fn test_write_and_read_note() {
        let store = MemoryStore::new();
        let repo = Repository::new(store);

        // Initially empty
        assert!(repo.list_notes().await.is_empty());
        assert!(repo.get_head().await.is_none());

        // Write a note
        repo.write_note("hello", "Hello World!", "markdown").await;

        // Should now have HEAD
        assert!(repo.get_head().await.is_some());

        // List notes
        let notes = repo.list_notes().await;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].name, "hello");
        assert_eq!(notes[0].path, "hello.md");
        assert_eq!(notes[0].note, "Hello World!");
        assert_eq!(notes[0].r#type, "markdown");
        assert!(notes[0].namespace.is_none());
    }

    #[tokio::test]
    async fn test_write_note_in_namespace() {
        let store = MemoryStore::new();
        let repo = Repository::new(store);

        repo.write_note("work/project", "Project notes", "markdown")
            .await;

        let notes = repo.list_notes().await;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].name, "project");
        assert_eq!(notes[0].path, "work/project.md");
        assert_eq!(notes[0].namespace, Some("work".to_string()));

        let namespaces = repo.list_namespaces().await;
        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].name, "work");
    }

    #[tokio::test]
    async fn test_get_note() {
        let store = MemoryStore::new();
        let repo = Repository::new(store);

        repo.write_note("test", "Test content", "text").await;

        let note = repo.get_note("test.txt").await.unwrap();
        assert_eq!(note.note, "Test content");
        assert_eq!(note.r#type, "text");
    }

    #[tokio::test]
    async fn test_delete_note() {
        let store = MemoryStore::new();
        let repo = Repository::new(store);

        repo.write_note("first", "First", "markdown").await;
        repo.write_note("second", "Second", "markdown").await;

        assert_eq!(repo.list_notes().await.len(), 2);

        repo.delete_note("first.md").await;

        let notes = repo.list_notes().await;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].name, "second");
    }

    #[tokio::test]
    async fn test_create_namespace() {
        let store = MemoryStore::new();
        let repo = Repository::new(store);

        repo.create_namespace("personal").await;

        let namespaces = repo.list_namespaces().await;
        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].name, "personal");
        assert!(namespaces[0].parent.is_none());
    }

    #[tokio::test]
    async fn test_config_roundtrip() {
        use crate::config::TypedNotesConfig;

        let store = MemoryStore::new();
        let repo = Repository::new(store);

        // Default config when nothing is stored
        let config = repo.get_config().await;
        assert_eq!(config, TypedNotesConfig::default());
        assert_eq!(config.notes.root, "");

        // Write a config
        let config = TypedNotesConfig::new("docs/notes".to_string());
        repo.set_config(&config).await;

        // Read it back
        let loaded = repo.get_config().await;
        assert_eq!(loaded.notes.root, "docs/notes");
    }

    #[tokio::test]
    async fn test_list_notes_in_subtree() {
        let store = MemoryStore::new();
        let repo = Repository::new(store);

        // Write notes at root and in a subfolder
        repo.write_note("root-note", "Root", "markdown").await;
        repo.write_note("docs/inner", "Inner", "markdown").await;
        repo.write_note("docs/sub/deep", "Deep", "text").await;

        // list_notes returns all
        assert_eq!(repo.list_notes().await.len(), 3);

        // list_notes_in("docs") returns only notes under docs/
        let docs_notes = repo.list_notes_in("docs").await;
        assert_eq!(docs_notes.len(), 2);
        assert!(docs_notes.iter().any(|n| n.name == "inner"));
        assert!(docs_notes.iter().any(|n| n.name == "deep"));

        // list_notes_in("") returns all (same as list_notes)
        assert_eq!(repo.list_notes_in("").await.len(), 3);

        // list_notes_in non-existent path returns empty
        assert!(repo.list_notes_in("nope").await.is_empty());
    }

    #[tokio::test]
    async fn test_list_namespaces_in_subtree() {
        let store = MemoryStore::new();
        let repo = Repository::new(store);

        repo.create_namespace("top").await;
        repo.write_note("docs/sub/note", "N", "markdown").await;

        // All namespaces
        let all = repo.list_namespaces().await;
        assert!(all.iter().any(|ns| ns.name == "top"));
        assert!(all.iter().any(|ns| ns.name == "docs"));

        // Namespaces under docs/
        let docs_ns = repo.list_namespaces_in("docs").await;
        assert_eq!(docs_ns.len(), 1);
        assert_eq!(docs_ns[0].name, "sub");
    }
}
