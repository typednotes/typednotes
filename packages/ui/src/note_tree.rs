use dioxus::prelude::*;
use store::{NamespaceInfo, TypedNoteInfo};

use crate::make_repo_for_user;

/// Centralized state for the note tree (notes + namespaces).
///
/// Provided as `Signal<NoteTree>` via context in `SidebarLayoutView`.
/// All views that need note/namespace data use `use_note_tree()` instead of
/// consuming two separate signals.
#[derive(Clone, Default)]
pub struct NoteTree {
    pub notes: Vec<TypedNoteInfo>,
    pub namespaces: Vec<NamespaceInfo>,
}

impl NoteTree {
    /// Reload notes and namespaces from the default (unscoped) local repository.
    pub async fn refresh() -> Self {
        Self::refresh_for(None).await
    }

    /// Reload notes and namespaces from a user-scoped local repository.
    ///
    /// Pass `Some("user-uuid")` to read from the user's isolated store,
    /// or `None` for the default unscoped store.
    pub async fn refresh_for(user_id: Option<&str>) -> Self {
        let repo = make_repo_for_user(user_id);
        NoteTree {
            notes: repo.list_notes().await,
            namespaces: repo.list_namespaces().await,
        }
    }
}

/// Consume the `Signal<NoteTree>` from context.
pub fn use_note_tree() -> Signal<NoteTree> {
    use_context::<Signal<NoteTree>>()
}
