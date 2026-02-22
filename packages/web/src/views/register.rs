//! Registration page view with email/password form.

use dioxus::prelude::*;
use ui::use_auth;

/// Register page component.
#[component]
pub fn Register() -> Element {
    let mut auth = use_auth();
    let mut name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);

    // If already logged in, redirect to notes
    if !auth().loading && auth().user.is_some() {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href("/notes");
            }
        }
    }

    let handle_register = move |evt: FormEvent| {
        evt.prevent_default();
        spawn(async move {
            error.set(None);

            let n = name().trim().to_string();
            let e = email().trim().to_string();
            let p = password();
            let cp = confirm_password();

            if n.is_empty() {
                error.set(Some("Name is required".to_string()));
                return;
            }
            if e.is_empty() || !e.contains('@') {
                error.set(Some("Please enter a valid email".to_string()));
                return;
            }
            if p.len() < 8 {
                error.set(Some("Password must be at least 8 characters".to_string()));
                return;
            }
            if p != cp {
                error.set(Some("Passwords do not match".to_string()));
                return;
            }

            loading.set(true);
            match api::register(e, p, n).await {
                Ok(user) => {
                    let mut state = auth();
                    state.user = Some(user);
                    state.loading = false;
                    auth.set(state);
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_href("/notes");
                        }
                    }
                }
                Err(e) => {
                    loading.set(false);
                    error.set(Some(e.to_string()));
                }
            }
        });
    };

    rsx! {
        div {
            class: "login-container",
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: 100vh; padding: 2rem; background: #ffffff;",

            h1 {
                style: "margin-bottom: 0.5rem; color: #37352f; font-weight: 700; font-size: 1.75rem;",
                "Create Account"
            }

            p {
                style: "margin-bottom: 2rem; color: #787774; font-size: 0.9375rem;",
                "Sign up for TypedNotes"
            }

            form {
                onsubmit: handle_register,
                style: "display: flex; flex-direction: column; gap: 0.75rem; width: 100%; max-width: 320px;",

                if let Some(err) = error() {
                    div {
                        style: "padding: 0.625rem; background: #fef2f2; border: 1px solid #fecaca; border-radius: 4px; color: #dc2626; font-size: 0.8125rem;",
                        "{err}"
                    }
                }

                input {
                    class: "auth-input",
                    r#type: "text",
                    placeholder: "Name",
                    value: name(),
                    oninput: move |evt| name.set(evt.value()),
                }

                input {
                    class: "auth-input",
                    r#type: "email",
                    placeholder: "Email",
                    value: email(),
                    oninput: move |evt| email.set(evt.value()),
                }

                input {
                    class: "auth-input",
                    r#type: "password",
                    placeholder: "Password (min 8 characters)",
                    value: password(),
                    oninput: move |evt| password.set(evt.value()),
                }

                input {
                    class: "auth-input",
                    r#type: "password",
                    placeholder: "Confirm password",
                    value: confirm_password(),
                    oninput: move |evt| confirm_password.set(evt.value()),
                }

                button {
                    class: "local-btn",
                    r#type: "submit",
                    disabled: loading(),
                    if loading() { "Creating account..." } else { "Sign up" }
                }
            }

            p {
                style: "margin-top: 1.5rem; font-size: 0.875rem; color: #787774;",
                "Already have an account? "
                a {
                    href: "/login",
                    style: "color: #2383e2; text-decoration: none;",
                    "Sign in"
                }
            }
        }

        style {
            r#"
            .auth-input {{
                width: 100%;
                padding: 0.625rem 0.75rem;
                border: 1px solid #e3e2e0;
                border-radius: 4px;
                font-size: 0.9375rem;
                color: #37352f;
                background: #ffffff;
                outline: none;
                box-sizing: border-box;
                font-family: inherit;
            }}

            .auth-input:focus {{
                border-color: #2383e2;
                box-shadow: 0 0 0 1px #2383e2;
            }}

            .local-btn {{
                padding: 0.625rem 1.25rem;
                background: #2383e2;
                color: white;
                border: none;
                border-radius: 4px;
                font-size: 0.9375rem;
                font-weight: 500;
                cursor: pointer;
                font-family: inherit;
                transition: background-color 0.15s;
            }}

            .local-btn:hover:not(:disabled) {{
                background: #1b6ec2;
            }}

            .local-btn:disabled {{
                opacity: 0.5;
                cursor: not-allowed;
            }}
            "#
        }
    }
}
