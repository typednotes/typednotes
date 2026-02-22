use dioxus::prelude::*;

use store::{NamespaceInfo, Repository, TypedNoteInfo};
use ui::{NewNoteDialog, NoteEditor, Sidebar, use_auth};

use crate::Route;

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
    // Decode path: "~" back to "/"
    let decoded_path = note_path.replace('~', "/");

    let mut notes = use_signal(Vec::<TypedNoteInfo>::new);
    let mut namespaces = use_signal(Vec::<NamespaceInfo>::new);
    let mut current_note = use_signal(|| Option::<TypedNoteInfo>::None);
    let mut show_new_note = use_signal(|| false);
    let mut new_note_namespace = use_signal(|| Option::<String>::None);
    let mut show_new_namespace = use_signal(|| false);
    let mut new_ns_name = use_signal(|| String::new());
    let nav = use_navigator();
    let auth = use_auth();

    // Clone for closures
    let decoded_path_for_save = decoded_path.clone();
    let decoded_path_for_delete = decoded_path.clone();
    let decoded_path_for_template = decoded_path.clone();

    // Load everything on mount and when path changes
    let _loader = use_resource(move || {
        let path = decoded_path.clone();
        async move {
            let repo = make_repo();
            notes.set(repo.list_notes().await);
            namespaces.set(repo.list_namespaces().await);
            current_note.set(repo.get_note(&path).await);
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

    let handle_save = {
        let decoded_path = decoded_path_for_save;
        move |content: String| {
            let path = decoded_path.clone();
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

                    // Fire-and-forget git sync
                    let sync_path = path.clone();
                    let sync_content = content.clone();
                    let sync_type = note.r#type.clone();
                    spawn(async move {
                        if let Err(_e) = api::sync_note(sync_path, sync_content, sync_type).await {
                            // Git sync failure is non-fatal; note is saved locally
                        }
                    });
                }
            });
        }
    };

    let handle_delete = {
        let decoded_path = decoded_path_for_delete;
        move |_| {
            let path = decoded_path.clone();
            spawn(async move {
                let repo = make_repo();
                repo.delete_note(&path).await;

                // Fire-and-forget git delete
                let del_path = path.clone();
                spawn(async move {
                    if let Err(_e) = api::delete_note_remote(del_path).await {
                        // Git delete sync failure is non-fatal
                    }
                });

                nav.push(Route::Notes {});
            });
        }
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

    rsx! {
        document::Stylesheet { href: NOTES_CSS }

        div {
            class: "notes-layout",

            Sidebar {
                namespaces: namespaces(),
                notes: notes(),
                active_path: Some(decoded_path_for_template.clone()),
                user: auth().user,
                on_select_note: on_select_note,
                on_create_note: on_create_note,
                on_create_namespace: on_create_namespace,
                on_navigate_settings: on_navigate_settings,
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
                    }
                } else {
                    div {
                        class: "notes-placeholder",
                        h2 { "Loading..." }
                    }
                }
            }
        }
    }
}
