//! Shared UI for the workspace: the dx components (`components`, from the
//! DioxusLabs/components registry), the navbar, sign-in, orgs, projects,
//! connections and messaging interfaces.

pub mod components;

mod navbar;
pub use navbar::Navbar;

mod auth;
pub use auth::{LoginPanel, UserMenu};

mod orgs;
pub use orgs::OrgsPanel;

mod org;
pub use org::OrgPage;

mod projects;
pub use projects::ProjectPage;

mod channels;
mod connections;
mod slug_form;

use dioxus::prelude::*;

/// The dx components' theme variables. Include once, at the app root.
pub const COMPONENTS_THEME: Asset = asset!("/assets/dx-components-theme.css");

/// The app's design tokens and page styles, on top of `COMPONENTS_THEME`.
/// Include once, at the app root, after it.
pub const APP_THEME: Asset = asset!("/assets/styling/theme.css");

/// A full-page navigation. The OAuth routes are plain HTTP redirects, not
/// router pages, and signing out must drop every cached server future — so
/// both leave the SPA rather than use the router. `url` is always built from
/// validated parts (fixed paths, provider ids, org and project slugs), never
/// user free text.
pub(crate) fn navigate_to(url: &str) {
    let url = url.replace(['\\', '\''], "");
    document::eval(&format!("window.location.assign('{url}')"));
}

/// A server function's error as a person should read it: the server's own
/// message, without the transport's "error running server function: …
/// (details: None)" wrapping.
pub(crate) fn error_message(e: &ServerFnError) -> String {
    match e {
        ServerFnError::ServerError { message, .. } => message.clone(),
        other => other.to_string(),
    }
}
