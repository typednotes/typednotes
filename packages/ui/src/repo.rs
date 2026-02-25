//! Shared repository constructor for all platforms.
//!
//! Returns a [`store::Repository`] backed by the appropriate [`store::ObjectStore`]:
//! - **Web** (WASM + `web` feature): IndexedDB via [`store::IdbStore`]
//! - **Desktop / Mobile** (native): filesystem via [`store::FileStore`]

/// Create a platform-appropriate repository.
///
/// On web this uses IndexedDB for browser-side persistence.
/// On desktop/mobile this uses a filesystem store under the OS data directory.
pub fn make_repo() -> store::Repository<impl store::ObjectStore> {
    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    {
        store::Repository::new(store::IdbStore::new())
    }
    #[cfg(not(all(target_arch = "wasm32", feature = "web")))]
    {
        let base = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("typednotes");
        store::Repository::new(store::FileStore::new(base))
    }
}
