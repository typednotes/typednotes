//! Registration page view with email/password form.

use dioxus::prelude::*;
use ui::components::{Button, ButtonVariant, Input};
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
            class: "flex flex-col items-center justify-center min-h-screen p-8 bg-white",

            h1 {
                class: "mb-2 text-neutral-800 font-bold text-[1.75rem]",
                "Create Account"
            }

            p {
                class: "mb-8 text-neutral-600 text-[0.9375rem]",
                "Sign up for TypedNotes"
            }

            form {
                onsubmit: handle_register,
                class: "flex flex-col gap-3 w-full max-w-[320px]",

                if let Some(err) = error() {
                    div {
                        class: "px-2.5 py-2.5 bg-red-50 border border-red-200 rounded text-red-600 text-[0.8125rem]",
                        "{err}"
                    }
                }

                Input {
                    class: "w-full",
                    r#type: "text",
                    placeholder: "Name",
                    value: name(),
                    oninput: move |evt: FormEvent| name.set(evt.value()),
                }

                Input {
                    class: "w-full",
                    r#type: "email",
                    placeholder: "Email",
                    value: email(),
                    oninput: move |evt: FormEvent| email.set(evt.value()),
                }

                Input {
                    class: "w-full",
                    r#type: "password",
                    placeholder: "Password (min 8 characters)",
                    value: password(),
                    oninput: move |evt: FormEvent| password.set(evt.value()),
                }

                Input {
                    class: "w-full",
                    r#type: "password",
                    placeholder: "Confirm password",
                    value: confirm_password(),
                    oninput: move |evt: FormEvent| confirm_password.set(evt.value()),
                }

                Button {
                    variant: ButtonVariant::Primary,
                    class: "w-full text-[0.9375rem] font-medium",
                    r#type: "submit",
                    disabled: loading(),
                    if loading() { "Creating account..." } else { "Sign up" }
                }
            }

            p {
                class: "mt-6 text-sm text-neutral-600",
                "Already have an account? "
                a {
                    class: "text-primary-500 no-underline",
                    href: "/login",
                    "Sign in"
                }
            }
        }
    }
}
