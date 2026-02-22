use api::UserInfo;
use dioxus::prelude::*;
use store::{NamespaceInfo, TypedNoteInfo};

const SIDEBAR_CSS: Asset = asset!("/assets/styling/sidebar.css");

#[component]
pub fn Sidebar(
    namespaces: Vec<NamespaceInfo>,
    notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    user: Option<UserInfo>,
    on_select_note: EventHandler<String>,
    on_create_note: EventHandler<Option<String>>,
    on_navigate_settings: EventHandler<()>,
) -> Element {
    // Build a tree: root namespaces + root notes
    let root_namespaces: Vec<&NamespaceInfo> =
        namespaces.iter().filter(|ns| ns.parent.is_none()).collect();
    let root_notes: Vec<&TypedNoteInfo> =
        notes.iter().filter(|n| n.namespace.is_none()).collect();

    rsx! {
        document::Stylesheet { href: SIDEBAR_CSS }

        div {
            class: "sidebar",

            // User header
            div {
                class: "sidebar-user",
                if let Some(ref u) = user {
                    if let Some(ref avatar) = u.avatar_url {
                        img {
                            class: "sidebar-user-avatar",
                            src: "{avatar}",
                            alt: "Avatar",
                        }
                    }
                    span {
                        class: "sidebar-user-name",
                        "{u.display_name()}"
                    }
                } else {
                    span {
                        class: "sidebar-user-name",
                        "TypedNotes"
                    }
                }
                button {
                    class: "sidebar-new-page",
                    title: "New page",
                    onclick: move |_| on_create_note.call(None),
                    "+"
                }
            }

            // Tree
            div {
                class: "sidebar-tree",

                // Root namespaces
                for ns in root_namespaces {
                    NamespaceNode {
                        key: "{ns.path}",
                        namespace: ns.clone(),
                        all_namespaces: namespaces.clone(),
                        all_notes: notes.clone(),
                        active_path: active_path.clone(),
                        on_select_note: on_select_note,
                        on_create_note: on_create_note,
                    }
                }

                // Root notes
                for note in root_notes {
                    div {
                        key: "{note.path}",
                        class: if active_path.as_ref() == Some(&note.path) { "note-item active" } else { "note-item" },
                        onclick: {
                            let path = note.path.clone();
                            move |_| on_select_note.call(path.clone())
                        },
                        span { class: "icon", "\u{1F4C4}" }
                        span { "{note.name}" }
                        span { class: "note-type-badge", "{note.r#type}" }
                    }
                }
            }

            // Bottom actions
            div {
                class: "sidebar-bottom",
                button {
                    class: "sidebar-bottom-item",
                    onclick: move |_| on_navigate_settings.call(()),
                    "Settings"
                }
                LogoutItem {}
            }
        }
    }
}

#[component]
fn LogoutItem() -> Element {
    let mut auth_state = crate::use_auth();

    let onclick = move |_| async move {
        if let Ok(()) = api::logout().await {
            auth_state.set(crate::AuthState {
                user: None,
                loading: false,
            });
            #[cfg(target_arch = "wasm32")]
            {
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/login");
                }
            }
        }
    };

    rsx! {
        button {
            class: "sidebar-bottom-item",
            onclick: onclick,
            "Log out"
        }
    }
}

#[component]
fn NamespaceNode(
    namespace: NamespaceInfo,
    all_namespaces: Vec<NamespaceInfo>,
    all_notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
    on_create_note: EventHandler<Option<String>>,
) -> Element {
    let mut expanded = use_signal(|| true);

    let child_namespaces: Vec<&NamespaceInfo> = all_namespaces
        .iter()
        .filter(|ns| ns.parent.as_ref() == Some(&namespace.path))
        .collect();
    let child_notes: Vec<&TypedNoteInfo> = all_notes
        .iter()
        .filter(|n| n.namespace.as_ref() == Some(&namespace.path))
        .collect();

    rsx! {
        div {
            class: "namespace-node",
            div {
                class: "namespace-header",
                onclick: move |_| expanded.set(!expanded()),
                span {
                    class: "icon",
                    if expanded() { "\u{25BE}" } else { "\u{25B8}" }
                }
                span { "{namespace.name}" }
            }

            if expanded() {
                div {
                    class: "namespace-children",
                    for child_ns in child_namespaces {
                        NamespaceNode {
                            key: "{child_ns.path}",
                            namespace: child_ns.clone(),
                            all_namespaces: all_namespaces.clone(),
                            all_notes: all_notes.clone(),
                            active_path: active_path.clone(),
                            on_select_note: on_select_note,
                            on_create_note: on_create_note,
                        }
                    }
                    for note in child_notes {
                        div {
                            key: "{note.path}",
                            class: if active_path.as_ref() == Some(&note.path) { "note-item active" } else { "note-item" },
                            onclick: {
                                let path = note.path.clone();
                                move |_| on_select_note.call(path.clone())
                            },
                            span { class: "icon", "\u{1F4C4}" }
                            span { "{note.name}" }
                            span { class: "note-type-badge", "{note.r#type}" }
                        }
                    }
                }
            }
        }
    }
}
