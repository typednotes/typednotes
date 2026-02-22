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
