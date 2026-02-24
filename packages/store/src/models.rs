//! # Domain models for notes and namespaces
//!
//! Defines the data structures returned by [`crate::Repository`] when listing or
//! reading the contents of a Git tree. These types are `Serialize + Deserialize`
//! so they can cross the server/client boundary via Dioxus server functions.
//!
//! ## Types
//!
//! | Struct | Represents |
//! |--------|-----------|
//! | [`TypedNoteInfo`] | A single note file in the repository. Carries the full tree path, a human-friendly `name` (filename without extension), an optional `namespace` (parent directory), the note `type` (`"markdown"` or `"text"`), the body content, and the blob SHA for change detection. |
//! | [`NamespaceInfo`] | A directory in the repository's note tree. Stores its full path, display name, and optional parent — used by the UI to render a folder hierarchy. |
//!
//! ## Helper functions
//!
//! - [`note_type_from_ext`] — maps a file extension to a note type (`"md"` → `"markdown"`,
//!   everything else → `"text"`).
//! - [`ext_from_note_type`] — the inverse mapping (`"markdown"` → `"md"`, default `"txt"`).
//!
//! These are used by [`crate::Repository`] when reading notes from the tree and when
//! constructing file paths for new or updated notes.

use serde::{Deserialize, Serialize};

/// Information about a note stored in the git tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypedNoteInfo {
    /// Full path in the tree: "work/project.md"
    pub path: String,
    /// Filename without extension: "project"
    pub name: String,
    /// Directory path or None for root: Some("work")
    pub namespace: Option<String>,
    /// Note type derived from extension: "markdown" or "text"
    pub r#type: String,
    /// Body content of the note
    pub note: String,
    /// Blob SHA hex string for change detection
    pub sha: String,
}

/// Information about a namespace (directory) in the git tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamespaceInfo {
    /// Full directory path: "work/ideas"
    pub path: String,
    /// Directory name: "ideas"
    pub name: String,
    /// Parent directory path or None for root: Some("work")
    pub parent: Option<String>,
}

/// Derive note type from file extension.
pub fn note_type_from_ext(ext: &str) -> &str {
    match ext {
        "md" => "markdown",
        "txt" => "text",
        _ => "text",
    }
}

/// Get the file extension for a note type.
pub fn ext_from_note_type(note_type: &str) -> &str {
    match note_type {
        "markdown" => "md",
        "text" => "txt",
        _ => "txt",
    }
}
