use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Input, Label, use_toast, ToastOptions};
use crate::{
    ActivityLogPanel, AppSidebar, NewNoteDialog, use_auth,
    NoteTree,
    LogLevel, log_activity, use_activity_log,
    SidebarProvider, SidebarInset, SidebarTrigger,
    SidebarCollapsible, SidebarVariant, use_sidebar,
};
use crate::components::sidebar::SidebarLayout as SidebarShell;
use crate::{make_repo_for_user};

use super::ModalOverlay;

/// Shared sidebar layout view.
///
/// Platform packages provide navigation callbacks and an `Outlet` as children.
/// Set `enable_git_pull` to `true` on web to trigger background git sync on load.
#[component]
pub fn SidebarLayoutView(
    /// Currently active note path (decoded, e.g. "folder/note.md"), if any.
    active_path: Option<String>,
    /// The router outlet for child routes.
    children: Element,
    /// Called when user selects a note. Receives the full note path (e.g. "folder/note.md").
    on_navigate_note: EventHandler<String>,
    /// Called when user clicks the settings button.
    on_navigate_settings: EventHandler<()>,
    /// Whether to run a background git pull on load (web only).
    #[props(default)]
    enable_git_pull: bool,
) -> Element {
    let mut tree: Signal<NoteTree> = use_context_provider(|| Signal::new(NoteTree::default()));

    let mut show_new_note = use_signal(|| false);
    let mut new_note_namespace = use_signal(|| Option::<String>::None);
    let mut show_new_namespace = use_signal(|| false);
    let mut new_ns_name = use_signal(|| String::new());
    let mut show_delete_ns = use_signal(|| false);
    let mut delete_ns_path = use_signal(|| String::new());
    let auth = use_auth();
    let mut activity_log = use_activity_log();

    // Load notes/namespaces from store + optional background git pull
    let _loader = use_resource(move || async move {
        let user_id = auth().user.as_ref().map(|u| u.id.clone());

        // Migrate legacy unscoped DB into user-scoped DB on first login (web only)
        #[cfg(all(target_arch = "wasm32", feature = "web"))]
        if let Some(ref uid) = user_id {
            let idb = store::IdbStore::with_namespace(Some(uid));
            idb.migrate_from_legacy_if_needed().await;
        }

        tree.set(NoteTree::refresh_for(user_id.as_deref()).await);

        if enable_git_pull {
            spawn(async move {
                log_activity(&mut activity_log, LogLevel::Info, "Pulling from git...");
                match api::pull_notes().await {
                    Ok(remote_files) => {
                        let user_id = auth().user.as_ref().map(|u| u.id.clone());
                        let repo = make_repo_for_user(user_id.as_deref());
                        let count = remote_files.len();
                        for file in &remote_files {
                            let ext = file.path.rsplit('.').next().unwrap_or("md");
                            let note_type = store::models::note_type_from_ext(ext);
                            let stem = file.path.trim_end_matches(&format!(".{ext}"));
                            repo.write_note(stem, &file.content, note_type).await;
                        }
                        if !remote_files.is_empty() {
                            let user_id = auth().user.as_ref().map(|u| u.id.clone());
                            tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
                        }
                        log_activity(&mut activity_log, LogLevel::Success, &format!("Pulled {count} notes"));
                    }
                    Err(e) => {
                        log_activity(&mut activity_log, LogLevel::Warning, &format!("Git pull: {e}"));
                        #[cfg(target_arch = "wasm32")]
                        web_sys::console::warn_1(&format!("Git pull: {e}").into());
                    }
                }
            });
        }
    });

    // Periodic sync timer (web only): pull remote changes at the configured interval
    #[cfg(target_arch = "wasm32")]
    {
        use_effect(move || {
            if !enable_git_pull {
                return;
            }
            spawn(async move {
                // Read sync interval from config
                let user_id = auth().user.as_ref().map(|u| u.id.clone());
                let repo = make_repo_for_user(user_id.as_deref());
                let config = repo.get_config().await;
                let interval_secs = config.sync.auto_sync_interval_secs;
                if interval_secs == 0 {
                    return;
                }
                loop {
                    gloo_timers::future::sleep(std::time::Duration::from_secs(interval_secs as u64)).await;
                    log_activity(&mut activity_log, LogLevel::Info, "Periodic pull...");
                    match api::pull_notes().await {
                        Ok(remote_files) => {
                            let user_id = auth().user.as_ref().map(|u| u.id.clone());
                            let repo = make_repo_for_user(user_id.as_deref());
                            for file in &remote_files {
                                let ext = file.path.rsplit('.').next().unwrap_or("md");
                                let note_type = store::models::note_type_from_ext(ext);
                                let stem = file.path.trim_end_matches(&format!(".{ext}"));
                                repo.write_note(stem, &file.content, note_type).await;
                            }
                            if !remote_files.is_empty() {
                                let user_id = auth().user.as_ref().map(|u| u.id.clone());
                                tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
                            }
                            log_activity(&mut activity_log, LogLevel::Success, &format!("Periodic pull: {} notes", remote_files.len()));
                        }
                        Err(e) => {
                            log_activity(&mut activity_log, LogLevel::Warning, &format!("Periodic pull: {e}"));
                        }
                    }
                }
            });
        });
    }

    // Sidebar callbacks
    let on_select_note = move |path: String| {
        show_new_note.set(false);
        show_new_namespace.set(false);
        on_navigate_note.call(path);
    };

    let on_create_note = move |ns: Option<String>| {
        new_note_namespace.set(ns);
        show_new_note.set(true);
        show_new_namespace.set(false);
        use_sidebar().set_open_mobile(false);
    };

    let on_create_namespace = move |parent: Option<String>| {
        let prefix = parent.map(|p| format!("{p}/")).unwrap_or_default();
        new_ns_name.set(prefix);
        show_new_namespace.set(true);
        show_new_note.set(false);
        use_sidebar().set_open_mobile(false);
    };

    let on_delete_namespace = move |path: String| {
        delete_ns_path.set(path);
        show_delete_ns.set(true);
    };

    let on_settings = move |_| {
        show_new_note.set(false);
        show_new_namespace.set(false);
        on_navigate_settings.call(());
    };

    // Handle creating a note from the dialog
    let toast = use_toast();
    let handle_create_note =
        move |(name, ns, note_type): (String, Option<String>, String)| {
            spawn(async move {
                let path = if let Some(ref ns) = ns {
                    format!("{ns}/{name}")
                } else {
                    name
                };
                let user_id = auth().user.as_ref().map(|u| u.id.clone());
                let repo = make_repo_for_user(user_id.as_deref());
                repo.write_note(&path, "", &note_type).await;
                tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
                show_new_note.set(false);
                let ext = store::models::ext_from_note_type(&note_type);
                let full_path = format!("{path}.{ext}");
                toast.success("Note created".to_string(), ToastOptions::new());
                on_navigate_note.call(full_path);
            });
        };

    // Handle creating a namespace
    let handle_create_namespace = move |_| {
        let name = new_ns_name().trim().to_string();
        if name.is_empty() {
            return;
        }
        spawn(async move {
            let user_id = auth().user.as_ref().map(|u| u.id.clone());
            let repo = make_repo_for_user(user_id.as_deref());
            repo.create_namespace(&name).await;
            tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
            show_new_namespace.set(false);
            toast.success("Folder created".to_string(), ToastOptions::new());
            // Sync namespace to remote
            if enable_git_pull {
                let _ = api::sync_namespace(name).await;
            }
        });
    };

    // Handle confirming namespace deletion
    let handle_confirm_delete_ns = move |_| {
        let path = delete_ns_path();
        spawn(async move {
            let user_id = auth().user.as_ref().map(|u| u.id.clone());
            let repo = make_repo_for_user(user_id.as_deref());
            repo.delete_namespace(&path).await;
            tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
            show_delete_ns.set(false);
            toast.success("Folder deleted".to_string(), ToastOptions::new());
            // Sync deletion to remote
            if enable_git_pull {
                match api::delete_namespace_remote(path.clone()).await {
                    Ok(()) => {
                        log_activity(&mut activity_log, LogLevel::Success, &format!("Deleted remote folder {path}"));
                    }
                    Err(e) => {
                        log_activity(&mut activity_log, LogLevel::Error, &format!("Delete folder sync error: {e}"));
                    }
                }
            }
        });
    };

    // Count items inside the namespace to show in the confirmation dialog
    let delete_ns_note_count = {
        let t = tree();
        let path = delete_ns_path();
        t.notes.iter().filter(|n| {
            n.namespace.as_ref().map_or(false, |ns| ns == &path || ns.starts_with(&format!("{path}/")))
        }).count()
    };
    let delete_ns_sub_count = {
        let t = tree();
        let path = delete_ns_path();
        t.namespaces.iter().filter(|ns| {
            ns.parent.as_ref().map_or(false, |p| p == &path || p.starts_with(&format!("{path}/")))
        }).count()
    };

    rsx! {
        SidebarProvider {
            SidebarShell {
                variant: SidebarVariant::Inset,
                collapsible: SidebarCollapsible::Offcanvas,
                AppSidebar {
                    namespaces: tree().namespaces,
                    notes: tree().notes,
                    active_path: active_path,
                    user: auth().user,
                    on_select_note: on_select_note,
                    on_create_note: on_create_note,
                    on_create_namespace: on_create_namespace,
                    on_delete_namespace: on_delete_namespace,
                    on_navigate_settings: on_settings,
                }
            }

            SidebarInset {
                // Top header with sidebar trigger
                header {
                    class: "flex items-center gap-2 px-4 py-2 border-b border-neutral-200",
                    SidebarTrigger {}
                    span { class: "text-sm font-semibold", "TypedNotes" }
                }

                // Main content
                div {
                    class: "flex-1 overflow-y-auto",
                    {children}
                }

                ActivityLogPanel {}
            }
        }

        // Modal overlays (always float on top)
        if show_new_note() {
            ModalOverlay {
                on_close: move |_| show_new_note.set(false),
                NewNoteDialog {
                    namespaces: tree().namespaces,
                    default_namespace: new_note_namespace(),
                    on_create: handle_create_note,
                    on_cancel: move |_| show_new_note.set(false),
                }
            }
        }
        if show_new_namespace() {
            ModalOverlay {
                on_close: move |_| show_new_namespace.set(false),
                div {
                    class: "p-6",
                    h2 { class: "m-0 mb-5 text-lg font-semibold text-neutral-800", "New Folder" }
                    div {
                        class: "mb-4",
                        Label { html_for: "new-folder-name", "Folder name" }
                        Input {
                            id: "new-folder-name",
                            class: "w-full mt-1.5",
                            r#type: "text",
                            placeholder: "my-folder",
                            value: new_ns_name(),
                            oninput: move |evt: FormEvent| new_ns_name.set(evt.value()),
                        }
                    }
                    div {
                        class: "flex gap-2 mt-5",
                        Button {
                            variant: ButtonVariant::Primary,
                            onclick: handle_create_namespace,
                            "Create"
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            onclick: move |_| show_new_namespace.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
        if show_delete_ns() {
            ModalOverlay {
                on_close: move |_| show_delete_ns.set(false),
                div {
                    class: "p-6",
                    h2 { class: "m-0 mb-5 text-lg font-semibold text-neutral-800", "Delete Folder" }
                    p {
                        class: "text-sm text-neutral-600 mb-2",
                        "Delete folder "
                        strong { "{delete_ns_path()}" }
                        " and all its contents?"
                    }
                    if delete_ns_note_count > 0 || delete_ns_sub_count > 0 {
                        p {
                            class: "text-xs text-neutral-500 mb-4",
                            "This folder contains {delete_ns_note_count} note(s) and {delete_ns_sub_count} sub-folder(s)."
                        }
                    }
                    div {
                        class: "flex gap-2 mt-5",
                        Button {
                            variant: ButtonVariant::Destructive,
                            onclick: handle_confirm_delete_ns,
                            "Delete"
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            onclick: move |_| show_delete_ns.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}
