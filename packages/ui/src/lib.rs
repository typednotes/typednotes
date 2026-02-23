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

pub mod activity_log;
pub use activity_log::{ActivityLog, LogLevel, log_activity, use_activity_log};

mod activity_log_panel;
pub use activity_log_panel::{ActivityLogPanel, ActivityLogToggle};
