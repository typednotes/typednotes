//! Shared UI for the workspace: the dx components (`components`, from the
//! DioxusLabs/components registry), the navbar, and the orgs panel.

pub mod components;

mod navbar;
pub use navbar::Navbar;

mod orgs;
pub use orgs::OrgsPanel;

use dioxus::prelude::*;

/// The dx components' theme variables. Include once, at the app root.
pub const COMPONENTS_THEME: Asset = asset!("/assets/dx-components-theme.css");
