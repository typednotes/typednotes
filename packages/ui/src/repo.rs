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

/// Wipe all local data for a user (scoped store) and anonymous store.
///
/// Call this when the user chooses to "detach" — it removes both the user's
/// scoped database and the anonymous database so they start fresh.
pub async fn detach_user(user_id: &str) {
    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    {
        store::IdbStore::delete_scoped(user_id).await;
        store::IdbStore::delete_anonymous().await;
    }
    #[cfg(not(all(target_arch = "wasm32", feature = "web")))]
    {
        let base = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("typednotes");
        store::FileStore::delete_scoped(&base, user_id);
        store::FileStore::delete_anonymous(&base);
    }
}

/// Migrate notes from the anonymous (unscoped) store to a user-scoped store (native only).
///
/// On desktop/mobile, anonymous notes live at `<data_dir>/typednotes/` while
/// user-scoped notes live at `<data_dir>/typednotes/<user_id>/`. This function
/// copies objects/ and refs/ from the anonymous store to the user store, then
/// removes the anonymous data.
///
/// Skips migration if the user store already has data or the anonymous store is empty.
#[cfg(not(all(target_arch = "wasm32", feature = "web")))]
pub async fn migrate_anonymous_to_user(user_id: &str) {
    let base = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("typednotes");
    let scoped = base.join(user_id);

    // Skip if user-scoped store already has refs (already has data)
    if scoped.join("refs").join("HEAD").exists() {
        return;
    }

    // Skip if anonymous store has no data
    if !base.join("refs").join("HEAD").exists() {
        return;
    }

    // Copy objects/
    let anon_objects = base.join("objects");
    let scoped_objects = scoped.join("objects");
    if anon_objects.is_dir() {
        let _ = std::fs::create_dir_all(&scoped_objects);
        if let Ok(entries) = std::fs::read_dir(&anon_objects) {
            for entry in entries.flatten() {
                let dest = scoped_objects.join(entry.file_name());
                let _ = std::fs::copy(entry.path(), dest);
            }
        }
    }

    // Copy refs/
    let anon_refs = base.join("refs");
    let scoped_refs = scoped.join("refs");
    if anon_refs.is_dir() {
        let _ = std::fs::create_dir_all(&scoped_refs);
        if let Ok(entries) = std::fs::read_dir(&anon_refs) {
            for entry in entries.flatten() {
                let dest = scoped_refs.join(entry.file_name());
                let _ = std::fs::copy(entry.path(), dest);
            }
        }
    }

    // Clean up anonymous store
    store::FileStore::delete_anonymous(&base);
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
