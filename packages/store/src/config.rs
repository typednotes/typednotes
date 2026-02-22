use serde::{Deserialize, Serialize};

/// Top-level configuration stored in `typednotes.toml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TypedNotesConfig {
    #[serde(default)]
    pub notes: NotesConfig,
}

/// Notes-specific configuration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NotesConfig {
    /// Subfolder within the repository that contains notes.
    /// Empty string means the repository root.
    #[serde(default)]
    pub root: String,
}

impl TypedNotesConfig {
    /// Create a config with the given notes root.
    pub fn new(root: String) -> Self {
        Self {
            notes: NotesConfig { root },
        }
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
