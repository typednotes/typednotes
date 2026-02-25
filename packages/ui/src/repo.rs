//! Shared repository constructor for all platforms.
//!
//! Returns a [`store::Repository`] backed by the appropriate [`store::ObjectStore`]:
//! - **Web** (WASM + `web` feature): IndexedDB via [`store::IdbStore`]
//! - **Desktop / Mobile** (native): filesystem via [`store::FileStore`]

/// Create a platform-appropriate repository (unscoped, default store).
///
/// Equivalent to `make_repo_for_user(None)`. Use this when no user identity
/// is available (e.g. unauthenticated or desktop/mobile without login).
pub fn make_repo() -> store::Repository<impl store::ObjectStore> {
    make_repo_for_user(None)
}

/// Create a platform-appropriate repository scoped to an optional user ID.
///
/// When `user_id` is `Some("uuid")`:
/// - **Web**: opens IndexedDB database `"typednotes-uuid"`
/// - **Desktop/Mobile**: uses filesystem path `<data_dir>/typednotes/uuid/`
///
/// When `user_id` is `None`, falls back to the default unscoped store.
pub fn make_repo_for_user(user_id: Option<&str>) -> store::Repository<impl store::ObjectStore> {
    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    {
        store::Repository::new(store::IdbStore::with_namespace(user_id))
    }
    #[cfg(not(all(target_arch = "wasm32", feature = "web")))]
    {
        let base = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("typednotes");
        let scoped = match user_id {
            Some(id) => base.join(id),
            None => base,
        };
        store::Repository::new(store::FileStore::new(scoped))
    }
}
