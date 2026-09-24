//! Shared UI for the workspace: the dx components (`components`, from the
//! DioxusLabs/components registry), the navbar, sign-in, orgs and
//! connections.

pub mod components;

mod navbar;
pub use navbar::Navbar;

mod auth;
pub use auth::{LoginPanel, UserMenu};

mod orgs;
pub use orgs::OrgsPanel;

mod org;
pub use org::OrgPage;

mod connections;

use dioxus::prelude::*;

/// The dx components' theme variables. Include once, at the app root.
pub const COMPONENTS_THEME: Asset = asset!("/assets/dx-components-theme.css");

/// A full-page navigation. The OAuth routes are plain HTTP redirects, not
/// router pages, and signing out must drop every cached server future — so
/// both leave the SPA rather than use the router. `url` is always built from
/// validated parts (fixed paths and org slugs), never user free text.
pub(crate) fn navigate_to(url: &str) {
    let url = url.replace(['\\', '\''], "");
    document::eval(&format!("window.location.assign('{url}')"));
}
