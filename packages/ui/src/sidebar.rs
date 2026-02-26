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
    FaFolder, FaFileLines, FaCaretLeft, FaCaretRight,
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

/// Item being dragged in the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub enum DragItem {
    Note { path: String },
    Namespace { path: String },
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
    /// Called when a note is dragged into a namespace: (note_path, target_namespace).
    /// target_namespace is None for root.
    #[props(default)]
    on_move_note: EventHandler<(String, Option<String>)>,
    /// Called when a namespace is dragged into another namespace: (ns_path, target_namespace).
    /// target_namespace is None for root.
    #[props(default)]
    on_move_namespace: EventHandler<(String, Option<String>)>,
) -> Element {
    let mut view_mode = use_signal(|| ViewMode::Flat);
    let mut flat_namespace = use_signal(|| Option::<String>::None);
    let mut slide_dir = use_signal(|| SlideDir::None);
    let mut nav_counter = use_signal(|| 0u32);
    let drag_item: Signal<Option<DragItem>> = use_signal(|| None);

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
                    title: "New namespace",
                    onclick: move |_| {
                        let ns = if view_mode() == ViewMode::Flat {
                            flat_namespace()
                        } else {
                            None
                        };
                        on_create_namespace.call(ns);
                    },
                    Icon { icon: FaFolderPlus }
                }
                button {
                    class: "sidebar-icon-btn",
                    title: "New note",
                    onclick: move |_| {
                        let ns = if view_mode() == ViewMode::Flat {
                            flat_namespace()
                        } else {
                            None
                        };
                        on_create_note.call(ns);
                    },
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
                            drag_item: drag_item,
                            on_move_note: on_move_note,
                            on_move_namespace: on_move_namespace,
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
                        drag_item: drag_item,
                        on_move_note: on_move_note,
                        on_move_namespace: on_move_namespace,
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
                            button {
                                onclick: move |_| {
                                    let mut log = crate::use_activity_log();
                                    let visible = log().visible;
                                    log.write().visible = !visible;
                                },
                                ..attrs,
                                Icon { icon: FaTerminal }
                                span {
                                    display: "flex",
                                    align_items: "center",
                                    gap: "0.5rem",
                                    "Activity Log"
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
    drag_item: Signal<Option<DragItem>>,
    on_move_note: EventHandler<(String, Option<String>)>,
    on_move_namespace: EventHandler<(String, Option<String>)>,
) -> Element {
    let root_namespaces: Vec<&NamespaceInfo> =
        namespaces.iter().filter(|ns| ns.parent.is_none()).collect();
    let root_notes: Vec<&TypedNoteInfo> =
        notes.iter().filter(|n| n.namespace.is_none()).collect();

    let mut root_drag_counter = use_signal(|| 0i32);

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
                drag_item: drag_item,
                on_move_note: on_move_note,
                on_move_namespace: on_move_namespace,
            }
        }

        for note in root_notes {
            NoteItem {
                key: "{note.path}",
                note: note.clone(),
                active_path: active_path.clone(),
                on_select_note: on_select_note,
                drag_item: drag_item,
            }
        }

        // Root drop zone — drop items here to move to root
        div {
            class: "sidebar-drop-root",
            "data-drag-over": if root_drag_counter() > 0 { "true" } else { "false" },
            ondragover: move |evt: Event<DragData>| {
                evt.prevent_default();
            },
            ondragenter: move |_| root_drag_counter += 1,
            ondragleave: move |_| root_drag_counter -= 1,
            ondrop: move |evt: Event<DragData>| {
                evt.prevent_default();
                root_drag_counter.set(0);
                if let Some(item) = drag_item() {
                    match item {
                        DragItem::Note { path } => on_move_note.call((path, None)),
                        DragItem::Namespace { path } => on_move_namespace.call((path, None)),
                    }
                }
                drag_item.set(None);
            },
            div {
                class: "px-2 py-1 text-xs opacity-30",
                "/ (root)"
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
    drag_item: Signal<Option<DragItem>>,
    on_move_note: EventHandler<(String, Option<String>)>,
    on_move_namespace: EventHandler<(String, Option<String>)>,
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
    let mut drag_counter = use_signal(|| 0i32);

    rsx! {
        // Wrapper div for drag events (components can't receive event handlers directly)
        div {
            draggable: "true",
            class: if drag_counter() > 0 { "sidebar-drop-active" } else { "" },
            ondragstart: {
                let ns_path = ns_path.clone();
                move |_| {
                    drag_item.set(Some(DragItem::Namespace { path: ns_path.clone() }));
                }
            },
            ondragend: move |_| drag_item.set(None),
            ondragover: move |evt: Event<DragData>| {
                evt.prevent_default();
            },
            ondragenter: move |_| drag_counter += 1,
            ondragleave: move |_| drag_counter -= 1,
            ondrop: {
                let target_ns = ns_path.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    drag_counter.set(0);
                    if let Some(item) = drag_item() {
                        match item {
                            DragItem::Note { path } => on_move_note.call((path, Some(target_ns.clone()))),
                            DragItem::Namespace { path } => {
                                if path != target_ns && !target_ns.starts_with(&format!("{path}/")) {
                                    on_move_namespace.call((path, Some(target_ns.clone())));
                                }
                            }
                        }
                    }
                    drag_item.set(None);
                }
            },
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
                                    title: "Delete namespace",
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
                                        on_create_namespace.call(Some(ns_path.clone()));
                                    },
                                    title: "New sub-namespace",
                                    ..attrs,
                                    Icon { icon: FaFolderPlus }
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
                                    title: "New note in namespace",
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
                                drag_item: drag_item,
                                on_move_note: on_move_note,
                                on_move_namespace: on_move_namespace,
                            }
                        }
                        for note in child_notes {
                            NoteSubItem {
                                key: "{note.path}",
                                note: note.clone(),
                                active_path: active_path.clone(),
                                on_select_note: on_select_note,
                                drag_item: drag_item,
                            }
                        }
                    }
                }
            }
        }
        } // close wrapper div
    }
}

#[component]
fn NoteItem(
    note: TypedNoteInfo,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
    drag_item: Signal<Option<DragItem>>,
) -> Element {
    let is_active = active_path.as_ref() == Some(&note.path);
    let path = note.path.clone();
    let path_for_drag = path.clone();
    let note_name = note.name.clone();
    let note_type = note.r#type.clone();

    rsx! {
        div {
            draggable: "true",
            ondragstart: move |_| {
                drag_item.set(Some(DragItem::Note { path: path_for_drag.clone() }));
            },
            ondragend: move |_| drag_item.set(None),
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
}

#[component]
fn NoteSubItem(
    note: TypedNoteInfo,
    active_path: Option<String>,
    on_select_note: EventHandler<String>,
    drag_item: Signal<Option<DragItem>>,
) -> Element {
    let is_active = active_path.as_ref() == Some(&note.path);
    let path = note.path.clone();
    let path_for_drag = path.clone();
    let note_name = note.name.clone();

    rsx! {
        div {
            draggable: "true",
            ondragstart: move |_| {
                drag_item.set(Some(DragItem::Note { path: path_for_drag.clone() }));
            },
            ondragend: move |_| drag_item.set(None),
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
        } // close wrapper div
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
    drag_item: Signal<Option<DragItem>>,
    on_move_note: EventHandler<(String, Option<String>)>,
    on_move_namespace: EventHandler<(String, Option<String>)>,
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

    let suffix = if nav_counter % 2 == 0 { "a" } else { "b" };
    let anim_class = match slide_dir {
        SlideDir::Right => format!("flat-view-enter-right-{suffix}"),
        SlideDir::Left => format!("flat-view-enter-left-{suffix}"),
        SlideDir::None => String::new(),
    };

    let mut breadcrumb_drag_counter = use_signal(|| 0i32);

    // Pre-compute parent namespace and breadcrumb label before rsx closures
    let parent_namespace = current_namespace.as_deref().and_then(|ns| {
        ns.rfind('/').map(|pos| ns[..pos].to_string())
    });
    let breadcrumb_label = current_namespace.as_deref().map(|label| {
        if let Some(pos) = label.rfind('/') {
            format!("../{}", &label[pos+1..])
        } else {
            format!("/ {label}")
        }
    });

    rsx! {
        div {
            class: "overflow-hidden",
            div {
                key: "{nav_counter}",
                class: "{anim_class}",
                // Breadcrumb / back button (also a drop target for moving to parent)
                if breadcrumb_label.is_some() {
                    div {
                        class: if breadcrumb_drag_counter() > 0 { "sidebar-drop-active" } else { "" },
                        ondragover: move |evt: Event<DragData>| {
                            evt.prevent_default();
                        },
                        ondragenter: move |_| breadcrumb_drag_counter += 1,
                        ondragleave: move |_| breadcrumb_drag_counter -= 1,
                        ondrop: {
                            let parent = parent_namespace.clone();
                            move |evt: Event<DragData>| {
                                evt.prevent_default();
                                breadcrumb_drag_counter.set(0);
                                if let Some(item) = drag_item() {
                                    match item {
                                        DragItem::Note { path } => on_move_note.call((path, parent.clone())),
                                        DragItem::Namespace { path } => {
                                            if parent.as_ref() != Some(&path) {
                                                on_move_namespace.call((path, parent.clone()));
                                            }
                                        }
                                    }
                                }
                                drag_item.set(None);
                            }
                        },
                        SidebarMenu {
                            SidebarMenuItem {
                                SidebarMenuButton {
                                    size: SidebarMenuButtonSize::Sm,
                                    tooltip: rsx! { "Go up" },
                                    as: {
                                        let label = breadcrumb_label.clone().unwrap_or_default();
                                        move |attrs: Vec<Attribute>| {
                                            let label = label.clone();
                                            rsx! {
                                                button {
                                                    onclick: move |_| on_navigate_up.call(()),
                                                    ..attrs,
                                                    Icon { icon: FaCaretLeft, width: 10, height: 10 }
                                                    span {
                                                        class: "text-xs opacity-70",
                                                        "{label}"
                                                    }
                                                }
                                            }
                                        }
                                    },
                                }
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
                        {
                            let ns_path = ns.path.clone();
                            let ns_name = ns.name.clone();
                            rsx! {
                                FlatNsItem {
                                    key: "{ns_path}",
                                    ns_path: ns_path,
                                    ns_name: ns_name,
                                    drag_item: drag_item,
                                    on_navigate_into: on_navigate_into,
                                    on_delete_namespace: on_delete_namespace,
                                    on_move_note: on_move_note,
                                    on_move_namespace: on_move_namespace,
                                }
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
                            let path_for_drag = path.clone();
                            rsx! {
                                div {
                                    key: "{path}",
                                    draggable: "true",
                                    ondragstart: move |_| {
                                        drag_item.set(Some(DragItem::Note { path: path_for_drag.clone() }));
                                    },
                                    ondragend: move |_| drag_item.set(None),
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
                                    } // close SidebarMenuItem
                                } // close wrapper div
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A single namespace row in the flat view — drag source + drop target.
#[component]
fn FlatNsItem(
    ns_path: String,
    ns_name: String,
    drag_item: Signal<Option<DragItem>>,
    on_navigate_into: EventHandler<String>,
    on_delete_namespace: EventHandler<String>,
    on_move_note: EventHandler<(String, Option<String>)>,
    on_move_namespace: EventHandler<(String, Option<String>)>,
) -> Element {
    let mut drag_counter = use_signal(|| 0i32);

    rsx! {
        div {
            draggable: "true",
            class: if drag_counter() > 0 { "sidebar-drop-active" } else { "" },
            ondragstart: {
                let ns_path = ns_path.clone();
                move |_| {
                    drag_item.set(Some(DragItem::Namespace { path: ns_path.clone() }));
                }
            },
            ondragend: move |_| drag_item.set(None),
            ondragover: move |evt: Event<DragData>| {
                evt.prevent_default();
            },
            ondragenter: move |_| drag_counter += 1,
            ondragleave: move |_| drag_counter -= 1,
            ondrop: {
                let target_ns = ns_path.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    drag_counter.set(0);
                    if let Some(item) = drag_item() {
                        match item {
                            DragItem::Note { path } => on_move_note.call((path, Some(target_ns.clone()))),
                            DragItem::Namespace { path } => {
                                if path != target_ns && !target_ns.starts_with(&format!("{path}/")) {
                                    on_move_namespace.call((path, Some(target_ns.clone())));
                                }
                            }
                        }
                    }
                    drag_item.set(None);
                }
            },
            SidebarMenuItem {
                SidebarMenuButton {
                    tooltip: rsx! { "{ns_name}" },
                    as: {
                        let ns_path = ns_path.clone();
                        let ns_name = ns_name.clone();
                        move |attrs: Vec<Attribute>| {
                            let ns_path = ns_path.clone();
                            let ns_name = ns_name.clone();
                            rsx! {
                                button {
                                    onclick: move |_| on_navigate_into.call(ns_path.clone()),
                                    ..attrs,
                                    Icon { icon: FaFolder, width: 12, height: 12 }
                                    span { "{ns_name}" }
                                    Icon { icon: FaCaretRight, width: 10, height: 10, class: "ml-auto opacity-40" }
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
                                    on_delete_namespace.call(ns_path.clone());
                                },
                                title: "Delete namespace",
                                ..attrs,
                                Icon { icon: FaTrashCan }
                            }
                        }
                    }
                },
            }
        }
        } // close wrapper div
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
