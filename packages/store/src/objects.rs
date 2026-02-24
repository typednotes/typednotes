//! # Git object model — types, serialisation, parsing, and hashing
//!
//! This module is a pure-Rust, dependency-light implementation of the four core
//! Git object types. It is used by [`crate::Repository`] to build and read
//! in-memory Git trees, and by [`api::git_transport`](../../api/src/git_transport.rs)
//! when packing/unpacking objects for the wire protocol.
//!
//! ## Object types
//!
//! | Struct | Git type | Description |
//! |--------|----------|-------------|
//! | [`Blob`] | `blob` | Raw file content (a note's body). |
//! | [`Tree`] | `tree` | A sorted directory listing of [`TreeEntry`] items, each carrying a mode, name, and child SHA. |
//! | [`Commit`] | `commit` | Points to a root [`Tree`] SHA, an optional parent commit, author/timestamp metadata, and a message. |
//! | [`Sha`] | — | A 20-byte SHA-1 hash that uniquely identifies any object. Supports hex round-tripping via [`Sha::from_hex`] / [`Sha::to_hex`]. |
//!
//! ## Hashing (write path)
//!
//! Each `hash_*` function serialises an object into the canonical Git format
//! (`"{type} {size}\0{content}"`), computes its SHA-1, and returns both the
//! [`Sha`] and the full byte representation ready to be stored in an object store.
//!
//! - [`hash_blob`] — wraps raw bytes with a `blob` header.
//! - [`hash_tree`] — sorts entries by name (with trailing `/` for directories,
//!   matching Git's collation), then encodes `"{mode} {name}\0{20-byte sha}"` per entry.
//! - [`hash_commit`] — produces the standard `tree`/`parent`/`author`/`committer`
//!   header block followed by a blank line and the commit message.
//!
//! ## Parsing (read path)
//!
//! The inverse `parse_*` functions take a raw stored object (including its header)
//! and return the corresponding struct, or `None` if the data is malformed or the
//! type tag does not match:
//!
//! - [`parse_blob`], [`parse_tree`], [`parse_commit`]
//!
//! All parsers delegate header validation to [`parse_header`], which checks the
//! type tag and verifies that the declared size matches the actual content length.

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

/// A 20-byte SHA-1 hash identifying a git object.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sha(pub [u8; 20]);

impl Sha {
    /// Create a Sha from a hex string.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 40 {
            return None;
        }
        let mut bytes = [0u8; 20];
        for i in 0..20 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Sha(bytes))
    }

    /// Return the hex string representation.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl std::fmt::Display for Sha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// A git blob (file content).
#[derive(Clone, Debug)]
pub struct Blob {
    pub content: Vec<u8>,
}

/// A single entry in a git tree.
#[derive(Clone, Debug)]
pub struct TreeEntry {
    pub mode: String,
    pub name: String,
    pub sha: Sha,
}

/// A git tree (directory listing).
#[derive(Clone, Debug)]
pub struct Tree {
    pub entries: Vec<TreeEntry>,
}

/// A git commit.
#[derive(Clone, Debug)]
pub struct Commit {
    pub tree: Sha,
    pub parent: Option<Sha>,
    pub author: String,
    pub message: String,
    pub timestamp: i64,
}

/// Hash raw data with a git object header: "{type} {size}\0{content}"
fn hash_with_header(obj_type: &str, content: &[u8]) -> (Sha, Vec<u8>) {
    let header = format!("{} {}\0", obj_type, content.len());
    let mut full = Vec::with_capacity(header.len() + content.len());
    full.extend_from_slice(header.as_bytes());
    full.extend_from_slice(content);

    let mut hasher = Sha1::new();
    hasher.update(&full);
    let result = hasher.finalize();
    let mut sha_bytes = [0u8; 20];
    sha_bytes.copy_from_slice(&result);

    (Sha(sha_bytes), full)
}

/// Serialize a blob and compute its SHA-1.
pub fn hash_blob(blob: &Blob) -> (Sha, Vec<u8>) {
    hash_with_header("blob", &blob.content)
}

/// Serialize a tree in git format and compute its SHA-1.
///
/// Git tree format: for each entry: "{mode} {name}\0{20-byte sha}"
pub fn hash_tree(tree: &Tree) -> (Sha, Vec<u8>) {
    let mut content = Vec::new();
    // Git trees require entries sorted by name
    let mut sorted_entries: Vec<&TreeEntry> = tree.entries.iter().collect();
    sorted_entries.sort_by(|a, b| {
        // Git sorts tree entries with a trailing '/' for directories
        let a_name = if a.mode == "40000" {
            format!("{}/", a.name)
        } else {
            a.name.clone()
        };
        let b_name = if b.mode == "40000" {
            format!("{}/", b.name)
        } else {
            b.name.clone()
        };
        a_name.cmp(&b_name)
    });

    for entry in sorted_entries {
        content.extend_from_slice(entry.mode.as_bytes());
        content.push(b' ');
        content.extend_from_slice(entry.name.as_bytes());
        content.push(0);
        content.extend_from_slice(&entry.sha.0);
    }
    hash_with_header("tree", &content)
}

/// Serialize a commit in git format and compute its SHA-1.
pub fn hash_commit(commit: &Commit) -> (Sha, Vec<u8>) {
    let mut content = String::new();
    content.push_str(&format!("tree {}\n", commit.tree.to_hex()));
    if let Some(ref parent) = commit.parent {
        content.push_str(&format!("parent {}\n", parent.to_hex()));
    }
    content.push_str(&format!(
        "author {} {} +0000\n",
        commit.author, commit.timestamp
    ));
    content.push_str(&format!(
        "committer {} {} +0000\n",
        commit.author, commit.timestamp
    ));
    content.push('\n');
    content.push_str(&commit.message);
    content.push('\n');

    hash_with_header("commit", content.as_bytes())
}

/// Parse a blob from raw git object data (after header).
pub fn parse_blob(raw: &[u8]) -> Option<Blob> {
    let (_, content) = parse_header(raw, "blob")?;
    Some(Blob {
        content: content.to_vec(),
    })
}

/// Parse a tree from raw git object data (after header).
pub fn parse_tree(raw: &[u8]) -> Option<Tree> {
    let (_, content) = parse_header(raw, "tree")?;
    let mut entries = Vec::new();
    let mut i = 0;

    while i < content.len() {
        // Find space after mode
        let space_pos = content[i..].iter().position(|&b| b == b' ')? + i;
        let mode = std::str::from_utf8(&content[i..space_pos]).ok()?;

        // Find null after name
        let null_pos = content[space_pos + 1..]
            .iter()
            .position(|&b| b == 0)?
            + space_pos
            + 1;
        let name = std::str::from_utf8(&content[space_pos + 1..null_pos]).ok()?;

        // Next 20 bytes are SHA
        if null_pos + 1 + 20 > content.len() {
            return None;
        }
        let mut sha_bytes = [0u8; 20];
        sha_bytes.copy_from_slice(&content[null_pos + 1..null_pos + 21]);

        entries.push(TreeEntry {
            mode: mode.to_string(),
            name: name.to_string(),
            sha: Sha(sha_bytes),
        });

        i = null_pos + 21;
    }

    Some(Tree { entries })
}

/// Parse a commit from raw git object data.
pub fn parse_commit(raw: &[u8]) -> Option<Commit> {
    let (_, content) = parse_header(raw, "commit")?;
    let text = std::str::from_utf8(content).ok()?;

    let mut tree = None;
    let mut parent = None;
    let mut author = String::new();
    let mut timestamp = 0i64;
    let mut in_headers = true;
    let mut message_lines = Vec::new();

    for line in text.lines() {
        if in_headers {
            if line.is_empty() {
                in_headers = false;
                continue;
            }
            if let Some(rest) = line.strip_prefix("tree ") {
                tree = Sha::from_hex(rest);
            } else if let Some(rest) = line.strip_prefix("parent ") {
                parent = Sha::from_hex(rest);
            } else if let Some(rest) = line.strip_prefix("author ") {
                // Format: "Name <email> timestamp +0000"
                if let Some(ts_start) = rest.rfind('>') {
                    author = rest[..=ts_start].to_string();
                    let after = rest[ts_start + 1..].trim();
                    if let Some(space) = after.find(' ') {
                        timestamp = after[..space].parse().unwrap_or(0);
                    }
                }
            }
        } else {
            message_lines.push(line);
        }
    }

    // Trim trailing empty lines from message
    while message_lines.last() == Some(&"") {
        message_lines.pop();
    }

    Some(Commit {
        tree: tree?,
        parent,
        author,
        message: message_lines.join("\n"),
        timestamp,
    })
}

/// Parse the header of a raw git object, returning (size, content_bytes).
fn parse_header<'a>(raw: &'a [u8], expected_type: &str) -> Option<(usize, &'a [u8])> {
    let null_pos = raw.iter().position(|&b| b == 0)?;
    let header = std::str::from_utf8(&raw[..null_pos]).ok()?;
    let (obj_type, size_str) = header.split_once(' ')?;
    if obj_type != expected_type {
        return None;
    }
    let size: usize = size_str.parse().ok()?;
    let content = &raw[null_pos + 1..];
    if content.len() != size {
        return None;
    }
    Some((size, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha_hex_roundtrip() {
        let hex = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let sha = Sha::from_hex(hex).unwrap();
        assert_eq!(sha.to_hex(), hex);
    }

    #[test]
    fn test_hash_blob() {
        // "hello\n" is a well-known git blob hash
        let blob = Blob {
            content: b"hello\n".to_vec(),
        };
        let (sha, _) = hash_blob(&blob);
        assert_eq!(sha.to_hex(), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    #[test]
    fn test_blob_roundtrip() {
        let blob = Blob {
            content: b"test content".to_vec(),
        };
        let (_, raw) = hash_blob(&blob);
        let parsed = parse_blob(&raw).unwrap();
        assert_eq!(parsed.content, blob.content);
    }

    #[test]
    fn test_tree_roundtrip() {
        let blob = Blob {
            content: b"hello".to_vec(),
        };
        let (blob_sha, _) = hash_blob(&blob);

        let tree = Tree {
            entries: vec![TreeEntry {
                mode: "100644".to_string(),
                name: "hello.txt".to_string(),
                sha: blob_sha,
            }],
        };
        let (_, raw) = hash_tree(&tree);
        let parsed = parse_tree(&raw).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].name, "hello.txt");
    }

    #[test]
    fn test_commit_roundtrip() {
        let sha = Sha::from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap();
        let commit = Commit {
            tree: sha.clone(),
            parent: None,
            author: "Test User <test@example.com>".to_string(),
            message: "initial commit".to_string(),
            timestamp: 1700000000,
        };
        let (_, raw) = hash_commit(&commit);
        let parsed = parse_commit(&raw).unwrap();
        assert_eq!(parsed.tree, commit.tree);
        assert_eq!(parsed.parent, None);
        assert_eq!(parsed.message, "initial commit");
    }
}
