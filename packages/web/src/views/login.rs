//! Login page view with OAuth buttons.

use dioxus::prelude::*;
use ui::{LoginButton, use_auth};

/// Login page component.
#[component]
pub fn Login() -> Element {
    let auth = use_auth();

    // If already logged in, redirect to notes
    if !auth().loading && auth().user.is_some() {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href("/notes");
            }
        }
    }

    rsx! {
        div {
            class: "login-container",
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: 100vh; padding: 2rem; background: #ffffff;",

            h1 {
                style: "margin-bottom: 0.5rem; color: #37352f; font-weight: 700; font-size: 1.75rem;",
                "TypedNotes"
            }

            p {
                style: "margin-bottom: 2rem; color: #787774; font-size: 0.9375rem;",
                "Choose your preferred sign-in method:"
            }

            div {
                class: "login-buttons",
                style: "display: flex; flex-direction: column; gap: 0.75rem; width: 100%; max-width: 320px;",

                LoginButton {
                    provider: "github",
                    label: "Continue with GitHub",
                    class: "login-btn github-btn",
                }

                LoginButton {
                    provider: "google",
                    label: "Continue with Google",
                    class: "login-btn google-btn",
                }
            }
        }

        style {
            r#"
            .login-btn {{
                display: flex;
                align-items: center;
                justify-content: center;
                padding: 0.625rem 1.25rem;
                border: none;
                border-radius: 4px;
                font-size: 0.9375rem;
                font-weight: 500;
                cursor: pointer;
                transition: background-color 0.15s;
                font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
            }}

            .login-btn:hover {{
                opacity: 0.9;
            }}

            .login-btn:disabled {{
                opacity: 0.5;
                cursor: not-allowed;
            }}

            .github-btn {{
                background-color: #24292e;
                color: white;
            }}

            .github-btn:hover:not(:disabled) {{
                background-color: #2f363d;
            }}

            .google-btn {{
                background-color: #4285f4;
                color: white;
            }}

            .google-btn:hover:not(:disabled) {{
                background-color: #357abd;
            }}
            "#
        }
    }
}
