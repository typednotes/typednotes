//! # Store crate — pure-Rust, platform-agnostic Git object store
//!
//! This crate implements TypedNotes' storage layer: a minimal Git-compatible
//! content-addressable store that works identically on the server (native) and in
//! the browser (WASM). It has **no dependency on the `git` binary** — every Git
//! concept (blobs, trees, commits, SHA-1 hashing, refs) is implemented from scratch.
//!
//! ## Module overview
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`objects`] | Core Git object types ([`Sha`], [`Blob`](objects::Blob), [`Tree`](objects::Tree), [`Commit`](objects::Commit)) with serialisation, parsing, and SHA-1 hashing. |
//! | [`repo`] | [`Repository`] — high-level async API for reading/writing notes and namespaces on top of an [`ObjectStore`]. |
//! | [`config`] | [`TypedNotesConfig`] — the `typednotes.toml` configuration (notes root, sync interval). |
//! | [`models`] | Domain types ([`TypedNoteInfo`], [`NamespaceInfo`]) returned by `Repository` queries. |
//! | [`memory`] | [`MemoryStore`] — `HashMap`-backed `ObjectStore` used server-side for transient Git sync and in tests. |
//! | [`idb`] | [`IdbStore`] — IndexedDB-backed `ObjectStore` for browser offline persistence (WASM + `web` feature only). |
//!
//! ## Platform gating
//!
//! - [`MemoryStore`] is always available (all targets).
//! - [`IdbStore`] is gated behind `#[cfg(all(target_arch = "wasm32", feature = "web"))]`,
//!   so it only compiles for the browser build.
//!
//! ## Re-exports
//!
//! The crate root re-exports the most commonly used types so that downstream crates
//! (`api`, `ui`) can write `use store::{Repository, MemoryStore, TypedNoteInfo, …}`
//! without reaching into submodules.

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
