pub mod config;
pub mod models;
pub mod objects;
pub mod repo;

mod memory;
pub use memory::MemoryStore;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod idb;
#[cfg(all(target_arch = "wasm32", feature = "web"))]
pub use idb::IdbStore;

pub use config::TypedNotesConfig;
pub use models::{NamespaceInfo, TypedNoteInfo};
pub use objects::Sha;
pub use repo::{ObjectStore, Repository};
