use api::{current_user, health, logout};
use dioxus::prelude::*;

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::navigate_to;

const AUTH_CSS: Asset = asset!("/assets/styling/auth.css");

/// Sign in with GitHub or Google. There is no sign-up form: the first
/// sign-in creates the account (docs/connections.md §2). Buttons for
/// providers this deployment has no OAuth client for are not shown.
#[component]
pub fn LoginPanel(error: Option<String>) -> Element {
    let status = use_server_future(health)?;
    let health = status().and_then(|r| r.ok());
    let (github, google) = health
        .as_ref()
        .map(|h| (h.github, h.google))
        .unwrap_or((false, false));

    rsx! {
        document::Link { rel: "stylesheet", href: AUTH_CSS }
        div { class: "auth",
            Card {
                CardHeader {
                    CardTitle { "Sign in to Typednotes" }
                    CardDescription {
                        "Your first sign-in creates your account. Accounts with the same verified email are the same user."
                    }
                }
                CardContent {
                    div { class: "auth-buttons",
                        if github {
                            Button {
                                onclick: move |_| navigate_to("/auth/github/login"),
                                "Continue with GitHub"
                            }
                        }
                        if google {
                            Button {
                                variant: ButtonVariant::Outline,
                                onclick: move |_| navigate_to("/auth/google/login"),
                                "Continue with Google"
                            }
                        }
                        if !github && !google && health.is_some() {
                            p { class: "auth-note",
                                "No sign-in provider is configured: set GITHUB_CLIENT_ID/GITHUB_CLIENT_SECRET or GOOGLE_CLIENT_ID/GOOGLE_CLIENT_SECRET."
                            }
                        }
                    }
                    if let Some(message) = error.filter(|e| !e.is_empty()) {
                        p { class: "auth-error", "{message}" }
                    }
                }
            }
        }
    }
}

/// The signed-in user and a sign-out button, for the navbar. Renders nothing
/// when signed out.
#[component]
pub fn UserMenu() -> Element {
    let me = use_server_future(current_user)?;
    let mut busy = use_signal(|| false);

    let Some(Ok(Some(user))) = me() else {
        return rsx! {};
    };
    let who = user
        .display_name
        .clone()
        .unwrap_or_else(|| user.email.clone());
    let initial = who
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    rsx! {
        document::Link { rel: "stylesheet", href: AUTH_CSS }
        div { class: "user-menu",
            span { class: "user-menu-avatar", aria_hidden: "true", "{initial}" }
            span { class: "user-menu-name", title: "{user.email}", "{who}" }
            Button {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                disabled: busy(),
                onclick: move |_| async move {
                    busy.set(true);
                    // Whatever the server says, leave: a failed request most
                    // likely means the session was already gone.
                    let _ = logout().await;
                    navigate_to("/");
                },
                "Sign out"
            }
        }
    }
}
