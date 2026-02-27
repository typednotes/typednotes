//! # Filesystem-backed object store
//!
//! [`FileStore`] is an [`ObjectStore`] implementation that persists Git objects
//! and refs to the local filesystem. It is used on desktop and mobile platforms
//! to retain notes across app restarts.
//!
//! ## Layout
//!
//! ```text
//! <base_dir>/
//! ├── objects/
//! │   └── <sha_hex>          # raw Git object bytes
//! └── refs/
//!     └── <ref_name>         # file containing the SHA hex string
//! ```
//!
//! ## Platform data directories
//!
//! Use [`dirs::data_dir()`] to obtain a platform-appropriate base:
//!
//! | Platform | Path |
//! |----------|------|
//! | macOS / iOS | `~/Library/Application Support/typednotes/` |
//! | Linux | `~/.local/share/typednotes/` |
//! | Windows | `C:\Users\<user>\AppData\Roaming\typednotes\` |
//! | Android | App-internal storage (via `dirs`) |

use std::path::PathBuf;

use crate::objects::Sha;
use crate::repo::ObjectStore;

/// Filesystem-backed ObjectStore for desktop and mobile persistence.
#[derive(Clone, Debug)]
pub struct FileStore {
    base: PathBuf,
}

impl FileStore {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    fn objects_dir(&self) -> PathBuf {
        self.base.join("objects")
    }

    fn refs_dir(&self) -> PathBuf {
        self.base.join("refs")
    }

    fn object_path(&self, sha: &Sha) -> PathBuf {
        self.objects_dir().join(sha.to_hex())
    }

    fn ref_path(&self, name: &str) -> PathBuf {
        self.refs_dir().join(name)
    }
}

impl FileStore {
    /// Delete a user-scoped store directory (`<base>/typednotes/<user_id>/`).
    pub fn delete_scoped(base: &std::path::Path, user_id: &str) {
        let scoped = base.join(user_id);
        let _ = std::fs::remove_dir_all(scoped);
    }

    /// Delete anonymous store data (objects/ and refs/ directly under `<base>/typednotes/`),
    /// without removing user-scoped subdirectories.
    pub fn delete_anonymous(base: &std::path::Path) {
        let _ = std::fs::remove_dir_all(base.join("objects"));
        let _ = std::fs::remove_dir_all(base.join("refs"));
    }
}

impl ObjectStore for FileStore {
    async fn get(&self, sha: &Sha) -> Option<Vec<u8>> {
        std::fs::read(self.object_path(sha)).ok()
    }

    async fn put(&self, sha: &Sha, data: Vec<u8>) {
        let path = self.object_path(sha);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, data);
    }

    async fn get_ref(&self, name: &str) -> Option<Sha> {
        let content = std::fs::read_to_string(self.ref_path(name)).ok()?;
        Sha::from_hex(content.trim())
    }

    async fn set_ref(&self, name: &str, sha: &Sha) {
        let path = self.ref_path(name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, sha.to_hex());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repository;

    #[tokio::test]
    async fn test_file_store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("typednotes_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let store = FileStore::new(dir.clone());
        let repo = Repository::new(store);

        // Write a note
        repo.write_note("hello", "Hello from FileStore!", "markdown")
            .await;

        // Re-open from same directory
        let store2 = FileStore::new(dir.clone());
        let repo2 = Repository::new(store2);

        let notes = repo2.list_notes().await;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].name, "hello");
        assert_eq!(notes[0].note, "Hello from FileStore!");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }
}
