//! Login page view with OAuth buttons.

use dioxus::prelude::*;
use ui::{LoginButton, use_auth};

/// Login page component.
#[component]
pub fn Login() -> Element {
    let auth = use_auth();

    // If already logged in, redirect to home
    if !auth().loading && auth().user.is_some() {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href("/");
            }
        }
    }

    rsx! {
        div {
            class: "login-container",
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: 60vh; padding: 2rem;",

            h1 {
                style: "margin-bottom: 2rem;",
                "Sign in to TypedNotes"
            }

            p {
                style: "margin-bottom: 2rem; color: #666;",
                "Choose your preferred sign-in method:"
            }

            div {
                class: "login-buttons",
                style: "display: flex; flex-direction: column; gap: 1rem; width: 100%; max-width: 300px;",

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
                padding: 0.75rem 1.5rem;
                border: none;
                border-radius: 6px;
                font-size: 1rem;
                font-weight: 500;
                cursor: pointer;
                transition: background-color 0.2s, transform 0.1s;
            }}

            .login-btn:hover {{
                transform: translateY(-1px);
            }}

            .login-btn:disabled {{
                opacity: 0.6;
                cursor: not-allowed;
                transform: none;
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
