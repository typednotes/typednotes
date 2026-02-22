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
