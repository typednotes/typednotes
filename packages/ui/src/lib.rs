//! This crate contains all shared UI for the workspace.

use dioxus::prelude::*;

pub mod components;

// Re-export icon library
pub use dioxus_free_icons::Icon;
pub mod icons {
    pub use dioxus_free_icons::icons::fa_solid_icons::*;
}

mod repo;
pub use repo::make_repo;

pub mod views;

pub const DX_COMPONENTS_CSS: Asset = asset!("/assets/dx-components-theme.css");
pub const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

mod navbar;
pub use navbar::Navbar;

mod auth;
pub use auth::{use_auth, AuthProvider, AuthState, LoginButton, LogoutButton};

mod sidebar;
pub use sidebar::{AppSidebar, ThemeSignal, load_theme_from_storage, apply_theme};

mod note_editor;
pub use note_editor::NoteEditor;

mod new_note_dialog;
pub use new_note_dialog::NewNoteDialog;

pub mod activity_log;
pub use activity_log::{ActivityLog, LogLevel, log_activity, use_activity_log};

mod activity_log_panel;
pub use activity_log_panel::{ActivityLogPanel, ActivityLogToggle};

// Re-export key sidebar component types for convenience
pub use components::sidebar::{
    SidebarProvider, SidebarInset, SidebarTrigger, SidebarRail,
    SidebarHeader, SidebarContent, SidebarFooter,
    SidebarGroup, SidebarGroupLabel, SidebarMenu, SidebarMenuItem,
    SidebarMenuButton, SidebarMenuButtonSize, SidebarMenuSub,
    SidebarMenuSubItem, SidebarMenuSubButton,
    SidebarVariant, SidebarCollapsible, SidebarSide,
    use_sidebar,
};
