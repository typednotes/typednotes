//! # Repository — high-level Git operations on an abstract object store
//!
//! This module is the core of TypedNotes' storage layer. [`Repository`] provides a
//! Git-compatible, content-addressable note store without requiring a working directory
//! or the `git` binary. All reads and writes go through the [`ObjectStore`] trait, so
//! the same logic works against an in-memory store (server-side Git sync), an
//! IndexedDB store (client-side offline), or any future backend.
//!
//! ## [`ObjectStore`] trait
//!
//! An async interface with four methods — `get`/`put` for SHA-keyed object blobs, and
//! `get_ref`/`set_ref` for named references (e.g. `"HEAD"`). Implementations live in
//! sibling modules ([`crate::memory`], [`crate::idb`]).
//!
//! ## Read path
//!
//! | Method | Description |
//! |--------|-------------|
//! | [`get_head`](Repository::get_head) | Returns the SHA the `HEAD` ref points to. |
//! | [`get_root_tree`](Repository::get_root_tree) | Follows `HEAD` → commit → root tree. |
//! | [`list_notes`](Repository::list_notes) | Recursively walks the root tree, collecting every `.md`/`.txt` blob as a [`TypedNoteInfo`]. |
//! | [`list_namespaces`](Repository::list_namespaces) | Same walk, but collects directories as [`NamespaceInfo`]. |
//! | [`list_notes_in`](Repository::list_notes_in) / [`list_namespaces_in`](Repository::list_namespaces_in) | Scoped variants that start from a subtree (e.g. a configured notes root). |
//! | [`get_note`](Repository::get_note) | Resolves a full path (e.g. `"work/ideas/project.md"`) to its blob and returns a [`TypedNoteInfo`]. |
//! | [`get_config`](Repository::get_config) | Reads `typednotes.toml` from the repo root, falling back to [`TypedNotesConfig::default`]. |
//!
//! ## Write path
//!
//! Every write method follows the same pattern: create or update the blob, rebuild
//! the tree hierarchy from the leaf up to the root (via [`update_tree_at_path`](Repository::update_tree_at_path)),
//! create a new commit pointing to the new root tree with the current `HEAD` as parent,
//! and advance `HEAD`. This mirrors how `git commit` works, but entirely in memory.
//!
//! | Method | Description |
//! |--------|-------------|
//! | [`write_note`](Repository::write_note) | Creates/updates a note, auto-appending the correct extension (`.md`/`.txt`). |
//! | [`write_note_raw`](Repository::write_note_raw) | Writes arbitrary bytes at an exact path (used internally for `.gitkeep` and config). |
//! | [`delete_note`](Repository::delete_note) | Removes a blob from the tree and commits the result. |
//! | [`create_namespace`](Repository::create_namespace) | Creates a directory by writing a `.gitkeep` file inside it. |
//! | [`set_config`](Repository::set_config) | Serialises a [`TypedNotesConfig`] to TOML and commits it at the repo root. |
//!
//! ## Tree manipulation
//!
//! [`update_tree_at_path`](Repository::update_tree_at_path) is the recursive workhorse:
//! given a path like `"work/ideas/project.md"`, it walks/creates intermediate subtrees
//! (`work` → `ideas`), inserts or removes the leaf entry, hashes every modified tree on
//! the way back up, and stores them in the object store. The result is a new root
//! [`Tree`] ready to be committed.
//!
//! ## Timestamps
//!
//! [`current_timestamp`] is platform-aware: it uses `js_sys::Date::now()` on WASM and
//! `std::time::SystemTime` on native, ensuring commits get sensible timestamps in both
//! environments.

use crate::config::TypedNotesConfig;
use crate::models::{ext_from_note_type, note_type_from_ext, NamespaceInfo, TypedNoteInfo};
use crate::objects::*;

/// Async trait for storing and retrieving git objects.
pub trait ObjectStore {
    fn get(
        &self,
        sha: &Sha,
    ) -> impl std::future::Future<Output = Option<Vec<u8>>>;
    fn put(
        &self,
        sha: &Sha,
        data: Vec<u8>,
    ) -> impl std::future::Future<Output = ()>;
    fn get_ref(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Option<Sha>>;
    fn set_ref(
        &self,
        name: &str,
        sha: &Sha,
    ) -> impl std::future::Future<Output = ()>;
}

/// A git repository backed by an ObjectStore.
pub struct Repository<S: ObjectStore> {
    store: S,
}

impl<S: ObjectStore> Repository<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Get the current HEAD commit SHA.
    pub async fn get_head(&self) -> Option<Sha> {
        self.store.get_ref("HEAD").await
    }

    /// Get the root tree of the current HEAD commit.
    async fn get_root_tree(&self) -> Option<Tree> {
        let head = self.get_head().await?;
        let commit_raw = self.store.get(&head).await?;
        let commit = parse_commit(&commit_raw)?;
        let tree_raw = self.store.get(&commit.tree).await?;
        parse_tree(&tree_raw)
    }

    /// List all notes in the repository.
    pub async fn list_notes(&self) -> Vec<TypedNoteInfo> {
        let mut notes = Vec::new();
        if let Some(tree) = self.get_root_tree().await {
            self.walk_tree_for_notes(&tree, "", &mut notes).await;
        }
        notes
    }

    /// Recursively walk a tree to find notes (files with .md or .txt extension).
    fn walk_tree_for_notes<'a>(
        &'a self,
        tree: &'a Tree,
        prefix: &'a str,
        notes: &'a mut Vec<TypedNoteInfo>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
        Box::pin(async move {
            for entry in &tree.entries {
                if entry.mode == "40000" {
                    // Directory — recurse
                    let sub_prefix = if prefix.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{}/{}", prefix, entry.name)
                    };
                    if let Some(raw) = self.store.get(&entry.sha).await {
                        if let Some(sub_tree) = parse_tree(&raw) {
                            self.walk_tree_for_notes(&sub_tree, &sub_prefix, notes)
                                .await;
                        }
                    }
                } else {
                    // File — check extension
                    let path = if prefix.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{}/{}", prefix, entry.name)
                    };

                    if let Some(ext) = entry.name.rsplit('.').next() {
                        if ext == "md" || ext == "txt" {
                            let name = entry.name
                                [..entry.name.len() - ext.len() - 1]
                                .to_string();
                            let namespace = if prefix.is_empty() {
                                None
                            } else {
                                Some(prefix.to_string())
                            };

                            // Read blob content
                            let note = if let Some(raw) = self.store.get(&entry.sha).await {
                                if let Some(blob) = parse_blob(&raw) {
                                    String::from_utf8(blob.content).unwrap_or_default()
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };

                            notes.push(TypedNoteInfo {
                                path,
                                name,
                                namespace,
                                r#type: note_type_from_ext(ext).to_string(),
                                note,
                                sha: entry.sha.to_hex(),
                            });
                        }
                    }
                }
            }
        })
    }

    /// List all namespaces (directories) in the repository.
    pub async fn list_namespaces(&self) -> Vec<NamespaceInfo> {
        let mut namespaces = Vec::new();
        if let Some(tree) = self.get_root_tree().await {
            self.walk_tree_for_namespaces(&tree, "", None, &mut namespaces)
                .await;
        }
        namespaces
    }

    /// Recursively walk a tree to find directories.
    fn walk_tree_for_namespaces<'a>(
        &'a self,
        tree: &'a Tree,
        prefix: &'a str,
        parent: Option<&'a str>,
        namespaces: &'a mut Vec<NamespaceInfo>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
        Box::pin(async move {
            for entry in &tree.entries {
                if entry.mode == "40000" {
                    let path = if prefix.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{}/{}", prefix, entry.name)
                    };

                    namespaces.push(NamespaceInfo {
                        path: path.clone(),
                        name: entry.name.clone(),
                        parent: parent.map(|s| s.to_string()),
                    });

                    if let Some(raw) = self.store.get(&entry.sha).await {
                        if let Some(sub_tree) = parse_tree(&raw) {
                            self.walk_tree_for_namespaces(
                                &sub_tree,
                                &path,
                                Some(&path),
                                namespaces,
                            )
                            .await;
                        }
                    }
                }
            }
        })
    }

    /// Get a note by its path. Returns (content, note_type).
    pub async fn get_note(&self, path: &str) -> Option<TypedNoteInfo> {
        let tree = self.get_root_tree().await?;
        let (blob_sha, _) = self.resolve_path(&tree, path).await?;

        let raw = self.store.get(&blob_sha).await?;
        let blob = parse_blob(&raw)?;
        let content = String::from_utf8(blob.content).ok()?;

        let filename = path.rsplit('/').next().unwrap_or(path);
        let ext = filename.rsplit('.').next().unwrap_or("txt");
        let name = filename[..filename.len() - ext.len() - 1].to_string();
        let namespace = if path.contains('/') {
            Some(path.rsplitn(2, '/').nth(1).unwrap_or("").to_string())
        } else {
            None
        };

        Some(TypedNoteInfo {
            path: path.to_string(),
            name,
            namespace,
            r#type: note_type_from_ext(ext).to_string(),
            note: content,
            sha: blob_sha.to_hex(),
        })
    }

    /// Resolve a path in a tree to (blob_sha, entry_name).
    fn resolve_path<'a>(
        &'a self,
        tree: &'a Tree,
        path: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<(Sha, String)>> + 'a>> {
        Box::pin(async move {
            let parts: Vec<&str> = path.splitn(2, '/').collect();
            match parts.as_slice() {
                [name] => {
                    // Leaf: find entry in current tree
                    tree.entries
                        .iter()
                        .find(|e| e.name == *name)
                        .map(|e| (e.sha.clone(), e.name.clone()))
                }
                [dir, rest] => {
                    // Intermediate: find subtree and recurse
                    let entry = tree.entries.iter().find(|e| e.name == *dir)?;
                    let raw = self.store.get(&entry.sha).await?;
                    let sub_tree = parse_tree(&raw)?;
                    self.resolve_path(&sub_tree, rest).await
                }
                _ => None,
            }
        })
    }

    /// Write a note at the given path with the specified content and type.
    /// Creates a new commit and returns the commit SHA.
    pub async fn write_note(
        &self,
        path: &str,
        content: &str,
        note_type: &str,
    ) -> Sha {
        // Ensure path has the right extension
        let ext = ext_from_note_type(note_type);
        let full_path = if path.ends_with(&format!(".{ext}")) {
            path.to_string()
        } else {
            format!("{path}.{ext}")
        };

        // Create blob
        let blob = Blob {
            content: content.as_bytes().to_vec(),
        };
        let (blob_sha, blob_raw) = hash_blob(&blob);
        self.store.put(&blob_sha, blob_raw).await;

        // Get or create root tree
        let root_tree = if let Some(tree) = self.get_root_tree().await {
            tree
        } else {
            Tree {
                entries: Vec::new(),
            }
        };

        // Update tree with new blob
        let new_root = self
            .update_tree_at_path(&root_tree, &full_path, Some(blob_sha))
            .await;
        let (tree_sha, tree_raw) = hash_tree(&new_root);
        self.store.put(&tree_sha, tree_raw).await;

        // Create commit
        let parent = self.get_head().await;
        let commit = Commit {
            tree: tree_sha,
            parent,
            author: "TypedNotes <notes@typednotes.com>".to_string(),
            message: format!("Update {full_path}"),
            timestamp: current_timestamp(),
        };
        let (commit_sha, commit_raw) = hash_commit(&commit);
        self.store.put(&commit_sha, commit_raw).await;
        self.store.set_ref("HEAD", &commit_sha).await;

        commit_sha
    }

    /// Delete a note at the given path. Returns the new commit SHA.
    pub async fn delete_note(&self, path: &str) -> Option<Sha> {
        let root_tree = self.get_root_tree().await?;

        let new_root = self.update_tree_at_path(&root_tree, path, None).await;
        let (tree_sha, tree_raw) = hash_tree(&new_root);
        self.store.put(&tree_sha, tree_raw).await;

        let parent = self.get_head().await;
        let commit = Commit {
            tree: tree_sha,
            parent,
            author: "TypedNotes <notes@typednotes.com>".to_string(),
            message: format!("Delete {path}"),
            timestamp: current_timestamp(),
        };
        let (commit_sha, commit_raw) = hash_commit(&commit);
        self.store.put(&commit_sha, commit_raw).await;
        self.store.set_ref("HEAD", &commit_sha).await;

        Some(commit_sha)
    }

    /// Create a namespace (directory) with a .gitkeep file.
    pub async fn create_namespace(&self, path: &str) -> Sha {
        let gitkeep_path = format!("{path}/.gitkeep");
        self.write_note_raw(&gitkeep_path, b"").await
    }

    /// Rename a note: reads content from old_path, writes to new_path, deletes old_path.
    pub async fn rename_note(&self, old_path: &str, new_path: &str) -> Option<Sha> {
        // Read existing content
        let note = self.get_note(old_path).await?;
        // Write to new path
        self.write_note(
            new_path.trim_end_matches(&format!(".{}", ext_from_note_type(&note.r#type))),
            &note.note,
            &note.r#type,
        )
        .await;
        // Delete old path
        self.delete_note(old_path).await
    }

    /// Read the `typednotes.toml` configuration from the repo root.
    pub async fn get_config(&self) -> TypedNotesConfig {
        let Some(tree) = self.get_root_tree().await else {
            return TypedNotesConfig::default();
        };
        let Some((blob_sha, _)) = self.resolve_path(&tree, TypedNotesConfig::filename()).await
        else {
            return TypedNotesConfig::default();
        };
        let Some(raw) = self.store.get(&blob_sha).await else {
            return TypedNotesConfig::default();
        };
        let Some(blob) = parse_blob(&raw) else {
            return TypedNotesConfig::default();
        };
        let Ok(text) = String::from_utf8(blob.content) else {
            return TypedNotesConfig::default();
        };
        TypedNotesConfig::from_toml(&text).unwrap_or_default()
    }

    /// Write the `typednotes.toml` configuration into the repo root.
    pub async fn set_config(&self, config: &TypedNotesConfig) -> Sha {
        let toml = config.to_toml().unwrap_or_default();
        self.write_note_raw(TypedNotesConfig::filename(), toml.as_bytes())
            .await
    }

    /// List notes scoped to a subtree (e.g. a notes root folder).
    /// If `root` is empty, behaves like `list_notes`.
    pub async fn list_notes_in(&self, root: &str) -> Vec<TypedNoteInfo> {
        let mut notes = Vec::new();
        let Some(root_tree) = self.get_root_tree().await else {
            return notes;
        };
        let tree = if root.is_empty() {
            root_tree
        } else {
            match self.resolve_subtree(&root_tree, root).await {
                Some(t) => t,
                None => return notes,
            }
        };
        self.walk_tree_for_notes(&tree, "", &mut notes).await;
        notes
    }

    /// List namespaces scoped to a subtree.
    /// If `root` is empty, behaves like `list_namespaces`.
    pub async fn list_namespaces_in(&self, root: &str) -> Vec<NamespaceInfo> {
        let mut namespaces = Vec::new();
        let Some(root_tree) = self.get_root_tree().await else {
            return namespaces;
        };
        let tree = if root.is_empty() {
            root_tree
        } else {
            match self.resolve_subtree(&root_tree, root).await {
                Some(t) => t,
                None => return namespaces,
            }
        };
        self.walk_tree_for_namespaces(&tree, "", None, &mut namespaces)
            .await;
        namespaces
    }

    /// Resolve a subtree path from a root tree.
    fn resolve_subtree<'a>(
        &'a self,
        tree: &'a Tree,
        path: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Tree>> + 'a>> {
        Box::pin(async move {
            let parts: Vec<&str> = path.splitn(2, '/').collect();
            match parts.as_slice() {
                [dir] => {
                    let entry = tree.entries.iter().find(|e| e.name == *dir && e.mode == "40000")?;
                    let raw = self.store.get(&entry.sha).await?;
                    parse_tree(&raw)
                }
                [dir, rest] => {
                    let entry = tree.entries.iter().find(|e| e.name == *dir && e.mode == "40000")?;
                    let raw = self.store.get(&entry.sha).await?;
                    let sub_tree = parse_tree(&raw)?;
                    self.resolve_subtree(&sub_tree, rest).await
                }
                _ => None,
            }
        })
    }

    /// Write raw bytes at a path.
    pub async fn write_note_raw(&self, path: &str, content: &[u8]) -> Sha {
        let blob = Blob {
            content: content.to_vec(),
        };
        let (blob_sha, blob_raw) = hash_blob(&blob);
        self.store.put(&blob_sha, blob_raw).await;

        let root_tree = if let Some(tree) = self.get_root_tree().await {
            tree
        } else {
            Tree {
                entries: Vec::new(),
            }
        };

        let new_root = self
            .update_tree_at_path(&root_tree, path, Some(blob_sha))
            .await;
        let (tree_sha, tree_raw) = hash_tree(&new_root);
        self.store.put(&tree_sha, tree_raw).await;

        let parent = self.get_head().await;
        let commit = Commit {
            tree: tree_sha,
            parent,
            author: "TypedNotes <notes@typednotes.com>".to_string(),
            message: format!("Create {path}"),
            timestamp: current_timestamp(),
        };
        let (commit_sha, commit_raw) = hash_commit(&commit);
        self.store.put(&commit_sha, commit_raw).await;
        self.store.set_ref("HEAD", &commit_sha).await;

        commit_sha
    }

    /// Update a tree by inserting or removing an entry at a path.
    /// If `blob_sha` is Some, inserts/updates. If None, removes.
    fn update_tree_at_path<'a>(
        &'a self,
        tree: &'a Tree,
        path: &'a str,
        blob_sha: Option<Sha>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Tree> + 'a>> {
        Box::pin(async move {
            let parts: Vec<&str> = path.splitn(2, '/').collect();
            let mut entries: Vec<TreeEntry> = tree.entries.clone();

            match parts.as_slice() {
                [filename] => {
                    if let Some(sha) = blob_sha {
                        // Insert or update entry
                        if let Some(existing) =
                            entries.iter_mut().find(|e| e.name == *filename)
                        {
                            existing.sha = sha;
                        } else {
                            entries.push(TreeEntry {
                                mode: "100644".to_string(),
                                name: filename.to_string(),
                                sha,
                            });
                        }
                    } else {
                        // Remove entry
                        entries.retain(|e| e.name != *filename);
                    }
                }
                [dir, rest] => {
                    // Find or create subtree
                    let sub_tree = if let Some(entry) =
                        entries.iter().find(|e| e.name == *dir)
                    {
                        if let Some(raw) = self.store.get(&entry.sha).await {
                            parse_tree(&raw).unwrap_or(Tree {
                                entries: Vec::new(),
                            })
                        } else {
                            Tree {
                                entries: Vec::new(),
                            }
                        }
                    } else {
                        Tree {
                            entries: Vec::new(),
                        }
                    };

                    let new_sub = self.update_tree_at_path(&sub_tree, rest, blob_sha).await;
                    let (sub_sha, sub_raw) = hash_tree(&new_sub);
                    self.store.put(&sub_sha, sub_raw).await;

                    if let Some(existing) = entries.iter_mut().find(|e| e.name == *dir) {
                        existing.sha = sub_sha;
                    } else {
                        entries.push(TreeEntry {
                            mode: "40000".to_string(),
                            name: dir.to_string(),
                            sha: sub_sha,
                        });
                    }
                }
                _ => {}
            }

            Tree { entries }
        })
    }
}

fn current_timestamp() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as i64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}
