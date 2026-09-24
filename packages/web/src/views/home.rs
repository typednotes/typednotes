use api::current_user;
use dioxus::prelude::*;
use ui::{LoginPanel, OrgsPanel};

/// Signed in: your orgs. Signed out: the sign-in panel.
#[component]
pub fn Home() -> Element {
    let me = use_server_future(current_user)?;
    match me() {
        Some(Ok(Some(_))) => rsx! { OrgsPanel {} },
        Some(Ok(None)) => rsx! { LoginPanel { error: None } },
        Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not check your session: {e}" } },
        None => rsx! { p { "Loading…" } },
    }
}

/// The sign-in page the OAuth callback redirects to on failure (`?error=…`).
#[component]
pub fn Login(error: String) -> Element {
    rsx! { LoginPanel { error: Some(error) } }
}
