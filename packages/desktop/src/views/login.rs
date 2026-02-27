//! Login page view for desktop with email/password and OAuth buttons.

use dioxus::prelude::*;
use ui::components::{Button, ButtonVariant, Input};
use ui::{LoginButton, use_auth};

use crate::Route;

/// Login page component for desktop.
#[component]
pub fn Login() -> Element {
    let mut auth = use_auth();
    let nav = use_navigator();
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);

    // If already logged in, redirect to notes
    if !auth().loading && auth().user.is_some() {
        nav.replace(Route::Notes {});
    }

    let handle_login = move |evt: FormEvent| {
        evt.prevent_default();
        spawn(async move {
            error.set(None);

            let e = email().trim().to_string();
            let p = password();

            if e.is_empty() {
                error.set(Some("Please enter your email".to_string()));
                return;
            }
            if p.is_empty() {
                error.set(Some("Please enter your password".to_string()));
                return;
            }

            loading.set(true);
            match api::login_password(e, p).await {
                Ok(user) => {
                    let mut state = auth();
                    state.user = Some(user);
                    state.loading = false;
                    auth.set(state);
                    nav.replace(Route::Notes {});
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
                "TypedNotes"
            }

            p {
                class: "mb-8 text-neutral-600 text-[0.9375rem]",
                "Sign in to your account"
            }

            // Email/password form
            form {
                onsubmit: handle_login,
                class: "flex flex-col gap-3 w-full max-w-[320px]",

                if let Some(err) = error() {
                    div {
                        class: "px-2.5 py-2.5 bg-red-50 border border-red-200 rounded text-red-600 text-[0.8125rem]",
                        "{err}"
                    }
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
                    placeholder: "Password",
                    value: password(),
                    oninput: move |evt: FormEvent| password.set(evt.value()),
                }

                Button {
                    variant: ButtonVariant::Primary,
                    class: "w-full text-[0.9375rem] font-medium",
                    r#type: "submit",
                    disabled: loading(),
                    if loading() { "Signing in..." } else { "Sign in" }
                }
            }

            // Divider
            div {
                class: "flex items-center gap-4 w-full max-w-[320px] my-6",
                div { class: "flex-1 h-px bg-neutral-300" }
                span { class: "text-neutral-600 text-[0.8125rem]", "or" }
                div { class: "flex-1 h-px bg-neutral-300" }
            }

            // OAuth buttons
            div {
                class: "flex flex-col gap-3 w-full max-w-[320px]",

                LoginButton {
                    provider: "github",
                    label: "Continue with GitHub",
                    class: "flex items-center justify-center px-5 py-2.5 border-none rounded text-[0.9375rem] font-medium cursor-pointer transition-colors duration-150 font-sans bg-[#24292e] text-white hover:bg-[#2f363d] disabled:opacity-50 disabled:cursor-not-allowed",
                }

                LoginButton {
                    provider: "google",
                    label: "Continue with Google",
                    class: "flex items-center justify-center px-5 py-2.5 border-none rounded text-[0.9375rem] font-medium cursor-pointer transition-colors duration-150 font-sans bg-[#4285f4] text-white hover:bg-[#357abd] disabled:opacity-50 disabled:cursor-not-allowed",
                }
            }

            p {
                class: "mt-6 text-sm text-neutral-600",
                "Don't have an account? "
                Link {
                    class: "text-primary-500 no-underline",
                    to: Route::Register {},
                    "Sign up"
                }
            }
        }
    }
}
