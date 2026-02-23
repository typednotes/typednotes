use api::UserInfo;
use dioxus::prelude::*;
use store::{NamespaceInfo, TypedNoteInfo};

const SIDEBAR_CSS: Asset = asset!("/assets/styling/sidebar.css");

#[derive(Clone, Debug, PartialEq)]
enum SidebarViewMode {
    DrillDown,
    Tree,
}

#[component]
pub fn Sidebar(
    namespaces: Vec<NamespaceInfo>,
    notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    user: Option<UserInfo>,
    on_select_note: EventHandler<String>,
    on_create_note: EventHandler<Option<String>>,
    on_create_namespace: EventHandler<Option<String>>,
    on_navigate_settings: EventHandler<()>,
    #[props(default = false)] collapsed: bool,
    #[props(default)] on_toggle_collapse: EventHandler<()>,
) -> Element {
    let mut view_mode = use_signal(|| SidebarViewMode::DrillDown);
    let mut current_folder = use_signal(|| Option::<String>::None);

    rsx! {
        document::Stylesheet { href: SIDEBAR_CSS }

        div {
            class: if collapsed { "sidebar sidebar-collapsed" } else { "sidebar" },

            // Toggle collapse button (always visible)
            div {
                class: "sidebar-toggle",
                button {
                    class: "sidebar-toggle-btn",
                    onclick: move |_| on_toggle_collapse.call(()),
                    title: if collapsed { "Expand sidebar" } else { "Collapse sidebar" },
                    if collapsed { "\u{25B6}" } else { "\u{25C0}" }
                }
            }

            if !collapsed {
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
                    // View mode toggle
                    button {
                        class: "sidebar-action-btn sidebar-view-toggle",
                        title: if view_mode() == SidebarViewMode::DrillDown { "Switch to tree view" } else { "Switch to drill-down view" },
                        onclick: move |_| {
                            if view_mode() == SidebarViewMode::DrillDown {
                                view_mode.set(SidebarViewMode::Tree);
                            } else {
                                view_mode.set(SidebarViewMode::DrillDown);
                                current_folder.set(None);
                            }
                        },
                        if view_mode() == SidebarViewMode::DrillDown { "\u{1F332}" } else { "\u{1F4CB}" }
                    }
                    button {
                        class: "sidebar-action-btn",
                        title: "New folder",
                        onclick: {
                            let cf = current_folder.clone();
                            move |_| on_create_namespace.call(cf())
                        },
                        "\u{1F4C1}"
                    }
                    button {
                        class: "sidebar-action-btn",
                        title: "New page",
                        onclick: {
                            let cf = current_folder.clone();
                            move |_| on_create_note.call(cf())
                        },
                        "+"
                    }
                }

                // Tree content
                div {
                    class: "sidebar-tree",

                    if view_mode() == SidebarViewMode::DrillDown {
                        DrillDownView {
                            namespaces: namespaces.clone(),
                            notes: notes.clone(),
                            active_path: active_path.clone(),
                            current_folder: current_folder,
                            on_select_note: on_select_note,
                            on_create_note: on_create_note,
                            on_create_namespace: on_create_namespace,
                        }
                    } else {
                        CompactTreeView {
                            namespaces: namespaces.clone(),
                            notes: notes.clone(),
                            active_path: active_path.clone(),
                            on_select_note: on_select_note,
                            on_create_note: on_create_note,
                            on_create_namespace: on_create_namespace,
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
}

// ---------------------------------------------------------------------------
// Drill-down mode
// ---------------------------------------------------------------------------

#[component]
fn DrillDownView(
    namespaces: Vec<NamespaceInfo>,
    notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    current_folder: Signal<Option<String>>,
    on_select_note: EventHandler<String>,
    on_create_note: EventHandler<Option<String>>,
    on_create_namespace: EventHandler<Option<String>>,
) -> Element {
    let folder = current_folder();

    // Filter: child namespaces of current_folder
    let child_namespaces: Vec<&NamespaceInfo> = namespaces
        .iter()
        .filter(|ns| ns.parent.as_ref() == folder.as_ref())
        .collect();

    // Filter: notes in current_folder
    let child_notes: Vec<&TypedNoteInfo> = notes
        .iter()
        .filter(|n| n.namespace.as_ref() == folder.as_ref())
        .collect();

    rsx! {
        // Back button + breadcrumb when inside a folder
        if let Some(ref folder_path) = folder {
            div {
                class: "sidebar-breadcrumb",
                button {
                    class: "sidebar-back-btn",
                    onclick: {
                        let fp = folder_path.clone();
                        move |_| {
                            // Go to parent: split on '/', take all but last
                            let parent = fp.rsplit_once('/').map(|(p, _)| p.to_string());
                            current_folder.set(parent);
                        }
                    },
                    "\u{2190}"
                }
                // Clickable breadcrumb segments
                {
                    let parts: Vec<&str> = folder_path.split('/').collect();
                    rsx! {
                        for (i, part) in parts.iter().enumerate() {
                            if i > 0 {
                                span { class: "breadcrumb-sep", " / " }
                            }
                            {
                                let target_path = parts[..=i].join("/");
                                rsx! {
                                    span {
                                        class: "breadcrumb-item",
                                        onclick: move |_| {
                                            current_folder.set(Some(target_path.clone()));
                                        },
                                        "{part}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Namespace items (folders)
        for ns in child_namespaces {
            div {
                key: "{ns.path}",
                class: "namespace-item",
                onclick: {
                    let path = ns.path.clone();
                    move |_| current_folder.set(Some(path.clone()))
                },
                span { class: "icon", "\u{1F4C1}" }
                span { class: "namespace-item-name", "{ns.name}" }
                span { class: "namespace-item-arrow", "\u{203A}" }
            }
        }

        // Note items
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

// ---------------------------------------------------------------------------
// Compact tree mode (original recursive tree)
// ---------------------------------------------------------------------------

#[component]
fn CompactTreeView(
    namespaces: Vec<NamespaceInfo>,
    notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
    on_create_note: EventHandler<Option<String>>,
    on_create_namespace: EventHandler<Option<String>>,
) -> Element {
    let root_namespaces: Vec<&NamespaceInfo> =
        namespaces.iter().filter(|ns| ns.parent.is_none()).collect();
    let root_notes: Vec<&TypedNoteInfo> =
        notes.iter().filter(|n| n.namespace.is_none()).collect();

    rsx! {
        for ns in root_namespaces {
            NamespaceNode {
                key: "{ns.path}",
                namespace: ns.clone(),
                all_namespaces: namespaces.clone(),
                all_notes: notes.clone(),
                active_path: active_path.clone(),
                on_select_note: on_select_note,
                on_create_note: on_create_note,
                on_create_namespace: on_create_namespace,
            }
        }

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
}

#[component]
fn NamespaceNode(
    namespace: NamespaceInfo,
    all_namespaces: Vec<NamespaceInfo>,
    all_notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
    on_create_note: EventHandler<Option<String>>,
    on_create_namespace: EventHandler<Option<String>>,
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
                span { class: "namespace-name", "{namespace.name}" }
                span {
                    class: "namespace-actions",
                    button {
                        class: "namespace-action-btn",
                        title: "New note in folder",
                        onclick: {
                            let path = namespace.path.clone();
                            move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                on_create_note.call(Some(path.clone()));
                            }
                        },
                        "+"
                    }
                    button {
                        class: "namespace-action-btn",
                        title: "New subfolder",
                        onclick: {
                            let path = namespace.path.clone();
                            move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                on_create_namespace.call(Some(path.clone()));
                            }
                        },
                        "\u{1F4C1}"
                    }
                }
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
                            on_create_namespace: on_create_namespace,
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
