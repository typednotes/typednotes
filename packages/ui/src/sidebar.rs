use api::UserInfo;
use dioxus::prelude::*;
use store::{NamespaceInfo, TypedNoteInfo};

use crate::activity_log_panel::ActivityLogToggle;
use crate::components::{
    Avatar, AvatarFallback, AvatarImage, AvatarImageSize,
    Badge, BadgeVariant,
    Collapsible, CollapsibleContent, CollapsibleTrigger,
    SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupLabel,
    SidebarHeader, SidebarMenu, SidebarMenuAction, SidebarMenuButton,
    SidebarMenuButtonSize, SidebarMenuItem, SidebarMenuSub, SidebarMenuSubButton,
    SidebarMenuSubItem, SidebarRail, SidebarSeparator,
};
use crate::Icon;
use crate::icons::{
    FaFolderPlus, FaPlus, FaList, FaFolderTree, FaGear, FaTerminal,
    FaFolder, FaFileLines, FaArrowLeft, FaChevronRight,
    FaCircleHalfStroke, FaMoon, FaSun, FaRightFromBracket,
    FaTrashCan,
};

#[derive(Clone, Copy, PartialEq)]
enum ViewMode {
    Tree,
    Flat,
}

#[derive(Clone, Copy, PartialEq)]
enum SlideDir {
    None,
    Left,
    Right,
}

/// The application-specific sidebar content (VS Code-style explorer).
/// Placed inside a `SidebarProvider` + `SidebarLayout` in the platform layouts.
#[component]
pub fn AppSidebar(
    namespaces: Vec<NamespaceInfo>,
    notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    user: Option<UserInfo>,
    on_select_note: EventHandler<String>,
    on_create_note: EventHandler<Option<String>>,
    on_create_namespace: EventHandler<Option<String>>,
    on_delete_namespace: EventHandler<String>,
    on_navigate_settings: EventHandler<()>,
) -> Element {
    let mut view_mode = use_signal(|| ViewMode::Tree);
    let mut flat_namespace = use_signal(|| Option::<String>::None);
    let mut slide_dir = use_signal(|| SlideDir::None);
    let mut nav_counter = use_signal(|| 0u32);

    rsx! {
        // ── Header: user info + action buttons ──
        SidebarHeader {
            div {
                class: "flex items-center gap-2",
                if let Some(ref u) = user {
                    if let Some(ref avatar_url) = u.avatar_url {
                        Avatar {
                            size: AvatarImageSize::Small,
                            class: "w-[22px] h-[22px] shrink-0",
                            AvatarImage {
                                src: "{avatar_url}",
                                alt: "Avatar",
                            }
                            AvatarFallback {
                                {u.display_name().chars().next().unwrap_or('?').to_string()}
                            }
                        }
                    }
                    span {
                        class: "flex-1 text-sm font-semibold overflow-hidden text-ellipsis whitespace-nowrap",
                        "{u.display_name()}"
                    }
                } else {
                    span {
                        class: "flex-1 text-sm font-semibold overflow-hidden text-ellipsis whitespace-nowrap",
                        "TypedNotes"
                    }
                }
            }
            div {
                class: "flex items-center gap-1",
                button {
                    class: "sidebar-icon-btn",
                    title: "New folder",
                    onclick: move |_| on_create_namespace.call(None),
                    Icon { icon: FaFolderPlus }
                }
                button {
                    class: "sidebar-icon-btn",
                    title: "New note",
                    onclick: move |_| on_create_note.call(None),
                    Icon { icon: FaPlus }
                }
            }
        }

        SidebarSeparator {}

        // ── Content: explorer tree or flat view ──
        SidebarContent {
            SidebarGroup {
                div {
                    class: "flex items-center justify-between",
                    SidebarGroupLabel { "EXPLORER" }
                    button {
                        class: "sidebar-icon-btn mr-2",
                        title: if view_mode() == ViewMode::Tree { "Switch to flat view" } else { "Switch to tree view" },
                        onclick: move |_| {
                            if view_mode() == ViewMode::Tree {
                                view_mode.set(ViewMode::Flat);
                                flat_namespace.set(None);
                                slide_dir.set(SlideDir::None);
                            } else {
                                view_mode.set(ViewMode::Tree);
                            }
                        },
                        if view_mode() == ViewMode::Tree {
                            Icon { icon: FaList, width: 10, height: 10 }
                        } else {
                            Icon { icon: FaFolderTree, width: 10, height: 10 }
                        }
                    }
                }
                if view_mode() == ViewMode::Tree {
                    SidebarMenu {
                        ExplorerTree {
                            namespaces: namespaces.clone(),
                            notes: notes.clone(),
                            active_path: active_path.clone(),
                            on_select_note: on_select_note,
                            on_create_note: on_create_note,
                            on_create_namespace: on_create_namespace,
                            on_delete_namespace: on_delete_namespace,
                        }
                    }
                } else {
                    FlatExplorerView {
                        nav_counter: nav_counter(),
                        current_namespace: flat_namespace(),
                        slide_dir: slide_dir(),
                        namespaces: namespaces.clone(),
                        notes: notes.clone(),
                        active_path: active_path.clone(),
                        on_select_note: on_select_note,
                        on_delete_namespace: on_delete_namespace,
                        on_navigate_into: move |ns: String| {
                            slide_dir.set(SlideDir::Right);
                            flat_namespace.set(Some(ns));
                            nav_counter += 1;
                        },
                        on_navigate_up: move |_| {
                            slide_dir.set(SlideDir::Left);
                            let current = flat_namespace();
                            if let Some(ref ns) = current {
                                if let Some(parent_end) = ns.rfind('/') {
                                    flat_namespace.set(Some(ns[..parent_end].to_string()));
                                } else {
                                    flat_namespace.set(None);
                                }
                            }
                            nav_counter += 1;
                        },
                    }
                }
            }
        }

        SidebarSeparator {}

        // ── Footer: settings, activity log, theme toggle, logout ──
        SidebarFooter {
            SidebarMenu {
                SidebarMenuItem {
                    SidebarMenuButton {
                        size: SidebarMenuButtonSize::Sm,
                        tooltip: rsx! { "Settings" },
                        as: move |attrs: Vec<Attribute>| rsx! {
                            button {
                                onclick: move |_| on_navigate_settings.call(()),
                                ..attrs,
                                Icon { icon: FaGear }
                                span { "Settings" }
                            }
                        },
                    }
                }
                SidebarMenuItem {
                    SidebarMenuButton {
                        size: SidebarMenuButtonSize::Sm,
                        tooltip: rsx! { "Activity Log" },
                        as: move |attrs: Vec<Attribute>| rsx! {
                            div { ..attrs,
                                Icon { icon: FaTerminal }
                                span { class: "flex items-center gap-2",
                                    ActivityLogToggle {}
                                }
                            }
                        },
                    }
                }
                SidebarMenuItem {
                    ThemeToggleItem {}
                }
                SidebarMenuItem {
                    LogoutItem {}
                }
            }
        }

        SidebarRail {}
    }
}

// ---------------------------------------------------------------------------
// Explorer tree (recursive namespace + note tree)
// ---------------------------------------------------------------------------

#[component]
fn ExplorerTree(
    namespaces: Vec<NamespaceInfo>,
    notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
    on_create_note: EventHandler<Option<String>>,
    on_create_namespace: EventHandler<Option<String>>,
    on_delete_namespace: EventHandler<String>,
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
                on_delete_namespace: on_delete_namespace,
            }
        }

        for note in root_notes {
            NoteItem {
                key: "{note.path}",
                note: note.clone(),
                active_path: active_path.clone(),
                on_select_note: on_select_note,
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
    on_delete_namespace: EventHandler<String>,
) -> Element {
    let child_namespaces: Vec<&NamespaceInfo> = all_namespaces
        .iter()
        .filter(|ns| ns.parent.as_ref() == Some(&namespace.path))
        .collect();
    let child_notes: Vec<&TypedNoteInfo> = all_notes
        .iter()
        .filter(|n| n.namespace.as_ref() == Some(&namespace.path))
        .collect();

    let ns_path = namespace.path.clone();

    rsx! {
        Collapsible {
            default_open: true,
            keep_mounted: true,
            SidebarMenuItem {
                CollapsibleTrigger {
                    as: {
                        let namespace_name = namespace.name.clone();
                        move |attrs: Vec<Attribute>| {
                            let namespace_name = namespace_name.clone();
                            rsx! {
                                SidebarMenuButton {
                                    attributes: attrs,
                                    tooltip: rsx! { "{namespace_name}" },
                                    Icon { icon: FaFolder, width: 12, height: 12 }
                                    span { "{namespace_name}" }
                                }
                            }
                        }
                    },
                }
                SidebarMenuAction {
                    show_on_hover: true,
                    as: {
                        let ns_path2 = ns_path.clone();
                        move |attrs: Vec<Attribute>| {
                            let ns_path2 = ns_path2.clone();
                            rsx! {
                                button {
                                    onclick: move |evt: Event<MouseData>| {
                                        evt.stop_propagation();
                                        on_delete_namespace.call(ns_path2.clone());
                                    },
                                    title: "Delete folder",
                                    ..attrs,
                                    Icon { icon: FaTrashCan }
                                }
                            }
                        }
                    },
                }
                SidebarMenuAction {
                    show_on_hover: true,
                    as: {
                        let ns_path = ns_path.clone();
                        move |attrs: Vec<Attribute>| {
                            let ns_path = ns_path.clone();
                            rsx! {
                                button {
                                    onclick: move |evt: Event<MouseData>| {
                                        evt.stop_propagation();
                                        on_create_note.call(Some(ns_path.clone()));
                                    },
                                    title: "New note in folder",
                                    ..attrs,
                                    Icon { icon: FaPlus }
                                }
                            }
                        }
                    },
                }
                CollapsibleContent {
                    SidebarMenuSub {
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
                                on_delete_namespace: on_delete_namespace,
                            }
                        }
                        for note in child_notes {
                            NoteSubItem {
                                key: "{note.path}",
                                note: note.clone(),
                                active_path: active_path.clone(),
                                on_select_note: on_select_note,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NoteItem(
    note: TypedNoteInfo,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
) -> Element {
    let is_active = active_path.as_ref() == Some(&note.path);
    let path = note.path.clone();
    let note_name = note.name.clone();
    let note_type = note.r#type.clone();

    rsx! {
        SidebarMenuItem {
            SidebarMenuButton {
                is_active: is_active,
                tooltip: rsx! { "{note_name}" },
                as: move |attrs: Vec<Attribute>| {
                    let path = path.clone();
                    let note_name = note_name.clone();
                    let note_type = note_type.clone();
                    rsx! {
                        button {
                            onclick: move |_| on_select_note.call(path.clone()),
                            ..attrs,
                            Icon { icon: FaFileLines, width: 12, height: 12 }
                            span { "{note_name}" }
                            Badge {
                                variant: BadgeVariant::Secondary,
                                class: "ml-auto text-[0.625rem]",
                                "{note_type}"
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn NoteSubItem(
    note: TypedNoteInfo,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
) -> Element {
    let is_active = active_path.as_ref() == Some(&note.path);
    let path = note.path.clone();
    let note_name = note.name.clone();

    rsx! {
        SidebarMenuSubItem {
            SidebarMenuSubButton {
                is_active: is_active,
                as: move |attrs: Vec<Attribute>| {
                    let path = path.clone();
                    let note_name = note_name.clone();
                    rsx! {
                        button {
                            onclick: move |_| on_select_note.call(path.clone()),
                            ..attrs,
                            Icon { icon: FaFileLines, width: 12, height: 12 }
                            span { "{note_name}" }
                        }
                    }
                },
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Flat explorer view (column-based file manager)
// ---------------------------------------------------------------------------

#[component]
fn FlatExplorerView(
    nav_counter: u32,
    current_namespace: Option<String>,
    slide_dir: SlideDir,
    namespaces: Vec<NamespaceInfo>,
    notes: Vec<TypedNoteInfo>,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
    on_delete_namespace: EventHandler<String>,
    on_navigate_into: EventHandler<String>,
    on_navigate_up: EventHandler<()>,
) -> Element {
    // Filter to direct children of current namespace
    let child_namespaces: Vec<&NamespaceInfo> = namespaces
        .iter()
        .filter(|ns| ns.parent.as_deref() == current_namespace.as_deref())
        .collect();
    let child_notes: Vec<&TypedNoteInfo> = notes
        .iter()
        .filter(|n| n.namespace.as_deref() == current_namespace.as_deref())
        .collect();

    let anim_class = match slide_dir {
        SlideDir::Right => "flat-view-enter-right",
        SlideDir::Left => "flat-view-enter-left",
        SlideDir::None => "",
    };

    rsx! {
        div {
            class: "overflow-hidden",
            div {
                key: "{nav_counter}",
                class: "{anim_class}",
                // Breadcrumb / back button
                if current_namespace.is_some() {
                    SidebarMenu {
                        SidebarMenuItem {
                            SidebarMenuButton {
                                size: SidebarMenuButtonSize::Sm,
                                tooltip: rsx! { "Go up" },
                                as: move |attrs: Vec<Attribute>| rsx! {
                                    button {
                                        onclick: move |_| on_navigate_up.call(()),
                                        ..attrs,
                                        Icon { icon: FaArrowLeft, width: 12, height: 12 }
                                        span {
                                            class: "text-xs opacity-70",
                                            {
                                                let label = current_namespace.as_deref().unwrap_or("");
                                                if let Some(pos) = label.rfind('/') {
                                                    format!("../{}", &label[pos+1..])
                                                } else {
                                                    format!("/ {label}")
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }
                } else {
                    div {
                        class: "px-2 py-1 text-xs opacity-50 font-medium",
                        "/ (root)"
                    }
                }

                SidebarMenu {
                    // Namespace folders
                    for ns in child_namespaces {
                        SidebarMenuItem {
                            key: "{ns.path}",
                            SidebarMenuButton {
                                tooltip: rsx! { "{ns.name}" },
                                as: {
                                    let ns_path = ns.path.clone();
                                    let ns_name = ns.name.clone();
                                    move |attrs: Vec<Attribute>| {
                                        let ns_path = ns_path.clone();
                                        let ns_name = ns_name.clone();
                                        rsx! {
                                            button {
                                                onclick: move |_| on_navigate_into.call(ns_path.clone()),
                                                ..attrs,
                                                Icon { icon: FaFolder, width: 12, height: 12 }
                                                span { "{ns_name}" }
                                                Icon { icon: FaChevronRight, width: 8, height: 8, class: "ml-auto opacity-40" }
                                            }
                                        }
                                    }
                                },
                            }
                            SidebarMenuAction {
                                show_on_hover: true,
                                as: {
                                    let ns_path = ns.path.clone();
                                    move |attrs: Vec<Attribute>| {
                                        let ns_path = ns_path.clone();
                                        rsx! {
                                            button {
                                                onclick: move |evt: Event<MouseData>| {
                                                    evt.stop_propagation();
                                                    on_delete_namespace.call(ns_path.clone());
                                                },
                                                title: "Delete folder",
                                                ..attrs,
                                                Icon { icon: FaTrashCan }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }

                    // Notes
                    for note in child_notes {
                        {
                            let is_active = active_path.as_ref() == Some(&note.path);
                            let path = note.path.clone();
                            let note_name = note.name.clone();
                            let note_type = note.r#type.clone();
                            rsx! {
                                SidebarMenuItem {
                                    key: "{path}",
                                    SidebarMenuButton {
                                        is_active: is_active,
                                        tooltip: rsx! { "{note_name}" },
                                        as: move |attrs: Vec<Attribute>| {
                                            let path = path.clone();
                                            let note_name = note_name.clone();
                                            let note_type = note_type.clone();
                                            rsx! {
                                                button {
                                                    onclick: move |_| on_select_note.call(path.clone()),
                                                    ..attrs,
                                                    Icon { icon: FaFileLines, width: 12, height: 12 }
                                                    span { "{note_name}" }
                                                    Badge {
                                                        variant: BadgeVariant::Secondary,
                                                        class: "ml-auto text-[0.625rem]",
                                                        "{note_type}"
                                                    }
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Theme toggle
// ---------------------------------------------------------------------------

/// Theme mode: None = system default, Some("dark") or Some("light")
pub type ThemeSignal = Signal<Option<String>>;

#[component]
fn ThemeToggleItem() -> Element {
    let theme = use_context::<ThemeSignal>();

    let (icon, label): (Element, &str) = match theme().as_deref() {
        None => (rsx! { Icon { icon: FaCircleHalfStroke } }, "Theme: System"),
        Some("dark") => (rsx! { Icon { icon: FaMoon } }, "Theme: Dark"),
        Some("light") => (rsx! { Icon { icon: FaSun } }, "Theme: Light"),
        _ => (rsx! { Icon { icon: FaCircleHalfStroke } }, "Theme: System"),
    };

    rsx! {
        SidebarMenuButton {
            size: SidebarMenuButtonSize::Sm,
            tooltip: rsx! { "Toggle theme" },
            as: move |attrs: Vec<Attribute>| {
                rsx! {
                    button {
                        onclick: move |_| {
                            let next = match theme().as_deref() {
                                None => Some("dark".to_string()),
                                Some("dark") => Some("light".to_string()),
                                _ => None,
                            };
                            apply_theme(next.as_deref());
                            let mut theme = theme;
                            theme.set(next);
                        },
                        ..attrs,
                        {icon.clone()}
                        span { "{label}" }
                    }
                }
            },
        }
    }
}

/// Apply theme to the document and persist to localStorage.
pub fn apply_theme(theme: Option<&str>) {
    #[cfg(target_arch = "wasm32")]
    {
        let js = match theme {
            Some(t) => format!(
                "document.documentElement.dataset.theme = '{t}'; localStorage.setItem('theme', '{t}');"
            ),
            None => "delete document.documentElement.dataset.theme; localStorage.removeItem('theme');".to_string(),
        };
        dioxus::prelude::document::eval(&js);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = theme;
}

/// Load theme from localStorage (call once on app startup).
pub fn load_theme_from_storage(theme: &mut ThemeSignal) {
    #[cfg(target_arch = "wasm32")]
    {
        let mut theme = *theme;
        spawn(async move {
            let result = dioxus::prelude::document::eval(
                "return localStorage.getItem('theme');"
            ).await;
            if let Ok(val) = result {
                let s = val.as_str().unwrap_or("").to_string();
                if s == "dark" || s == "light" {
                    apply_theme(Some(&s));
                    theme.set(Some(s));
                }
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = theme;
}

#[component]
fn LogoutItem() -> Element {
    let mut auth_state = crate::use_auth();

    rsx! {
        SidebarMenuButton {
            size: SidebarMenuButtonSize::Sm,
            tooltip: rsx! { "Log out" },
            as: move |attrs: Vec<Attribute>| rsx! {
                button {
                    onclick: move |_| async move {
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
                    },
                    ..attrs,
                    Icon { icon: FaRightFromBracket }
                    span { "Log out" }
                }
            },
        }
    }
}
