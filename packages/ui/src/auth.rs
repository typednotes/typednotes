//! Authentication context and hooks for the UI.

use api::UserInfo;
use dioxus::prelude::*;

/// Authentication state for the application.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthState {
    pub user: Option<UserInfo>,
    pub loading: bool,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            user: None,
            loading: true,
        }
    }
}

/// Get the current authentication state.
/// Returns a signal that updates when the user logs in or out.
pub fn use_auth() -> Signal<AuthState> {
    use_context::<Signal<AuthState>>()
}

/// Provider component that manages authentication state.
/// Wrap your app with this component to enable authentication.
#[component]
pub fn AuthProvider(children: Element) -> Element {
    let mut auth_state = use_signal(AuthState::default);

    // Fetch the current user on mount
    let _ = use_resource(move || async move {
        match api::get_current_user().await {
            Ok(user) => {
                auth_state.set(AuthState {
                    user,
                    loading: false,
                });
            }
            Err(_) => {
                auth_state.set(AuthState {
                    user: None,
                    loading: false,
                });
            }
        }
    });

    use_context_provider(|| auth_state);

    rsx! {
        {children}
    }
}

/// Button to initiate login with a specific provider.
#[component]
pub fn LoginButton(
    provider: String,
    #[props(default = "Login".to_string())] label: String,
    #[props(default = "".to_string())] class: String,
) -> Element {
    let provider_clone = provider.clone();
    let loading = use_signal(|| false);

    let onclick = move |_| {
        let provider = provider_clone.clone();
        async move {
            loading.set(true);
            match api::get_login_url(provider).await {
                Ok(url) => {
                    // Redirect to OAuth provider
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_href(&url);
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = url;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to get login URL: {}", e);
                    loading.set(false);
                }
            }
        }
    };

    rsx! {
        button {
            class: "{class}",
            disabled: loading(),
            onclick: onclick,
            if loading() {
                "Loading..."
            } else {
                "{label}"
            }
        }
    }
}

/// Button to log out the current user.
#[component]
pub fn LogoutButton(
    #[props(default = "Logout".to_string())] label: String,
    #[props(default = "".to_string())] class: String,
) -> Element {
    let mut auth_state = use_auth();

    let onclick = move |_| async move {
        if let Ok(()) = api::logout().await {
            auth_state.set(AuthState {
                user: None,
                loading: false,
            });
            // Redirect to home
            #[cfg(target_arch = "wasm32")]
            {
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/");
                }
            }
        }
    };

    rsx! {
        button {
            class: "{class}",
            onclick: onclick,
            "{label}"
        }
    }
}
