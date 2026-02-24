//! # Repository-level configuration — `typednotes.toml`
//!
//! Defines the TOML configuration file that lives at the root of a TypedNotes
//! Git repository (filename: [`TypedNotesConfig::filename`] = `"typednotes.toml"`).
//! The file is read during sync to determine how notes are organised in the repo
//! and how automatic synchronisation behaves.
//!
//! ## Structure
//!
//! ```toml
//! [notes]
//! root = "notes"          # subfolder containing notes (empty = repo root)
//!
//! [sync]
//! auto_sync_interval_secs = 30   # 0 to disable auto-sync
//! ```
//!
//! ## Types
//!
//! | Struct | Purpose |
//! |--------|---------|
//! | [`TypedNotesConfig`] | Top-level config. Provides builder helpers (`new`, `with_sync_interval`), TOML (de)serialisation, and the canonical filename constant. |
//! | [`NotesConfig`] | Notes section — currently just a `root` path for the notes subfolder. |
//! | [`SyncConfig`] | Sync section — `auto_sync_interval_secs` with a default of **30 seconds**. |
//!
//! All structs derive `Default` (with sensible production defaults) so that a
//! missing or empty config file is equivalent to the default configuration.

use serde::{Deserialize, Serialize};

/// Top-level configuration stored in `typednotes.toml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TypedNotesConfig {
    #[serde(default)]
    pub notes: NotesConfig,
    #[serde(default)]
    pub sync: SyncConfig,
}

/// Notes-specific configuration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NotesConfig {
    /// Subfolder within the repository that contains notes.
    /// Empty string means the repository root.
    #[serde(default)]
    pub root: String,
}

/// Sync configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Auto-sync interval in seconds. 0 disables auto-sync.
    #[serde(default = "default_auto_sync_interval")]
    pub auto_sync_interval_secs: u32,
}

fn default_auto_sync_interval() -> u32 {
    30
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            auto_sync_interval_secs: default_auto_sync_interval(),
        }
    }
}

impl TypedNotesConfig {
    /// Create a config with the given notes root.
    pub fn new(root: String) -> Self {
        Self {
            notes: NotesConfig { root },
            sync: SyncConfig::default(),
        }
    }

    /// Builder method to set auto-sync interval.
    pub fn with_sync_interval(mut self, secs: u32) -> Self {
        self.sync.auto_sync_interval_secs = secs;
        self
    }

    /// The well-known filename for the config file.
    pub fn filename() -> &'static str {
        "typednotes.toml"
    }

    /// Parse from TOML string.
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Serialize to TOML string.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}
