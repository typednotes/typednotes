use dioxus::prelude::*;

use store::{NamespaceInfo, Repository, TypedNoteInfo};
use ui::{ActivityLogPanel, NewNoteDialog, NoteEditor, Sidebar, use_auth, LogLevel, log_activity, use_activity_log};

use crate::{Route, SidebarState};

const NOTES_CSS: Asset = asset!("/assets/notes.css");

fn make_repo() -> Repository<impl store::ObjectStore> {
    #[cfg(target_arch = "wasm32")]
    {
        Repository::new(store::IdbStore::new())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Repository::new(store::MemoryStore::new())
    }
}

#[component]
pub fn NoteDetail(note_path: String) -> Element {
    // Track decoded path in a signal so use_resource re-runs on route param change
    let mut path_signal = use_signal(|| note_path.replace('~', "/"));
    if *path_signal.peek() != note_path.replace('~', "/") {
        path_signal.set(note_path.replace('~', "/"));
    }

    let mut notes = use_signal(Vec::<TypedNoteInfo>::new);
    let mut namespaces = use_signal(Vec::<NamespaceInfo>::new);
    let mut current_note = use_signal(|| Option::<TypedNoteInfo>::None);
    let mut auto_sync_secs = use_signal(|| 30u32);
    let mut show_new_note = use_signal(|| false);
    let mut new_note_namespace = use_signal(|| Option::<String>::None);
    let mut show_new_namespace = use_signal(|| false);
    let mut new_ns_name = use_signal(|| String::new());
    let nav = use_navigator();
    let auth = use_auth();

    // Load everything on mount and when path changes
    let _loader = use_resource(move || {
        let path = path_signal(); // Reads signal → reactive dependency
        async move {
            let repo = make_repo();
            notes.set(repo.list_notes().await);
            namespaces.set(repo.list_namespaces().await);
            current_note.set(repo.get_note(&path).await);
            let config = repo.get_config().await;
            auto_sync_secs.set(config.sync.auto_sync_interval_secs);
        }
    });

    let on_select_note = move |path: String| {
        let encoded = path.replace('/', "~");
        nav.push(Route::NoteDetail {
            note_path: encoded,
        });
    };

    let on_create_note = move |ns: Option<String>| {
        new_note_namespace.set(ns);
        show_new_note.set(true);
        show_new_namespace.set(false);
    };

    let on_create_namespace = move |parent: Option<String>| {
        let prefix = parent.map(|p| format!("{p}/")).unwrap_or_default();
        new_ns_name.set(prefix);
        show_new_namespace.set(true);
        show_new_note.set(false);
    };

    let handle_create_namespace = move |_| {
        let name = new_ns_name().trim().to_string();
        if name.is_empty() {
            return;
        }
        spawn(async move {
            let repo = make_repo();
            repo.create_namespace(&name).await;
            notes.set(repo.list_notes().await);
            namespaces.set(repo.list_namespaces().await);
            show_new_namespace.set(false);
        });
    };

    let on_navigate_settings = move |_| {
        nav.push(Route::Settings {});
    };

    let mut activity_log = use_activity_log();

    let handle_save = move |content: String| {
        let path = path_signal();
        spawn(async move {
            let repo = make_repo();
            if let Some(note) = current_note() {
                let stem = path.trim_end_matches(&format!(
                    ".{}",
                    store::models::ext_from_note_type(&note.r#type)
                ));
                repo.write_note(stem, &content, &note.r#type).await;
                current_note.set(repo.get_note(&path).await);
                notes.set(repo.list_notes().await);
                log_activity(&mut activity_log, LogLevel::Info, &format!("Saved {path}"));

                // Git sync
                match api::sync_note(path.clone(), content.clone(), note.r#type.clone()).await {
                    Ok(()) => log_activity(&mut activity_log, LogLevel::Success, &format!("Synced {path}")),
                    Err(e) => {
                        log_activity(&mut activity_log, LogLevel::Error, &format!("Sync error: {e}"));
                        #[cfg(target_arch = "wasm32")]
                        web_sys::console::warn_1(&format!("Git sync: {e}").into());
                    }
                }
            }
        });
    };

    let handle_delete = move |_| {
        let path = path_signal();
        spawn(async move {
            let repo = make_repo();
            repo.delete_note(&path).await;
            log_activity(&mut activity_log, LogLevel::Info, &format!("Deleted {path}"));

            // Git delete
            match api::delete_note_remote(path.clone()).await {
                Ok(()) => log_activity(&mut activity_log, LogLevel::Success, &format!("Deleted remote {path}")),
                Err(e) => {
                    log_activity(&mut activity_log, LogLevel::Error, &format!("Delete sync error: {e}"));
                    #[cfg(target_arch = "wasm32")]
                    web_sys::console::warn_1(&format!("Git delete sync: {e}").into());
                }
            }

            nav.push(Route::Notes {});
        });
    };

    let handle_create_note =
        move |(name, ns, note_type): (String, Option<String>, String)| {
            spawn(async move {
                let path = if let Some(ref ns) = ns {
                    format!("{ns}/{name}")
                } else {
                    name
                };
                let repo = make_repo();
                repo.write_note(&path, "", &note_type).await;
                notes.set(repo.list_notes().await);
                namespaces.set(repo.list_namespaces().await);
                show_new_note.set(false);
                let ext = store::models::ext_from_note_type(&note_type);
                let full_path = format!("{path}.{ext}");
                let encoded = full_path.replace('/', "~");
                nav.push(Route::NoteDetail {
                    note_path: encoded,
                });
            });
        };

    let mut sidebar_state = use_context::<Signal<SidebarState>>();
    let mut is_resizing = use_signal(|| false);

    let on_toggle_collapse = move |_| {
        let mut st = sidebar_state.write();
        st.collapsed = !st.collapsed;
    };

    let handle_mouse_move = move |evt: Event<MouseData>| {
        if is_resizing() {
            let x = evt.page_coordinates().x;
            let new_width = x.max(120.0).min(600.0);
            sidebar_state.write().width = new_width;
        }
    };

    let handle_mouse_up = move |_| {
        is_resizing.set(false);
    };

    let ss = sidebar_state();
    let sidebar_width = if ss.collapsed { "48px".to_string() } else { format!("{}px", ss.width) };

    rsx! {
        document::Stylesheet { href: NOTES_CSS }

        div {
            class: "notes-layout",
            onmousemove: handle_mouse_move,
            onmouseup: handle_mouse_up,

            div {
                style: "width: {sidebar_width}; min-width: {sidebar_width}; display: flex; flex-shrink: 0;",

                Sidebar {
                    namespaces: namespaces(),
                    notes: notes(),
                    active_path: Some(path_signal()),
                    user: auth().user,
                    on_select_note: on_select_note,
                    on_create_note: on_create_note,
                    on_create_namespace: on_create_namespace,
                    on_navigate_settings: on_navigate_settings,
                    collapsed: ss.collapsed,
                    on_toggle_collapse: on_toggle_collapse,
                }

                if !ss.collapsed {
                    div {
                        class: if is_resizing() { "sidebar-resize-handle active" } else { "sidebar-resize-handle" },
                        onmousedown: move |_| is_resizing.set(true),
                    }
                }
            }

            div {
                class: "notes-main",

                if show_new_note() {
                    NewNoteDialog {
                        namespaces: namespaces(),
                        default_namespace: new_note_namespace(),
                        on_create: handle_create_note,
                        on_cancel: move |_| show_new_note.set(false),
                    }
                } else if show_new_namespace() {
                    div {
                        class: "new-note-form",
                        h2 { "New Folder" }
                        div {
                            class: "form-field",
                            label { "Folder name" }
                            input {
                                r#type: "text",
                                placeholder: "my-folder",
                                value: new_ns_name(),
                                oninput: move |evt| new_ns_name.set(evt.value()),
                            }
                        }
                        div {
                            class: "form-actions",
                            button {
                                class: "primary",
                                onclick: handle_create_namespace,
                                "Create"
                            }
                            button {
                                class: "secondary",
                                onclick: move |_| show_new_namespace.set(false),
                                "Cancel"
                            }
                        }
                    }
                } else if let Some(note) = current_note() {
                    NoteEditor {
                        key: "{note.sha}",
                        note: note.clone(),
                        breadcrumb: note.namespace.clone(),
                        on_save: handle_save,
                        on_delete: handle_delete,
                        auto_sync_interval_secs: auto_sync_secs(),
                    }
                } else {
                    div {
                        class: "notes-placeholder",
                        h2 { "Loading..." }
                    }
                }

                ActivityLogPanel {}
            }
        }
    }
}
