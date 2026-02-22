//! This crate contains all shared UI for the workspace.

mod navbar;
pub use navbar::Navbar;

mod auth;
pub use auth::{use_auth, AuthProvider, AuthState, LoginButton, LogoutButton};

mod sidebar;
pub use sidebar::Sidebar;

mod note_editor;
pub use note_editor::NoteEditor;

mod new_note_dialog;
pub use new_note_dialog::NewNoteDialog;
