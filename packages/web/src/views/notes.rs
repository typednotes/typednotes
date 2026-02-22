use dioxus::prelude::*;

use store::{NamespaceInfo, Repository, TypedNoteInfo};
use ui::{NewNoteDialog, Sidebar, use_auth};

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
pub fn Notes() -> Element {
    let mut notes = use_signal(Vec::<TypedNoteInfo>::new);
    let mut namespaces = use_signal(Vec::<NamespaceInfo>::new);
    let mut show_new_note = use_signal(|| false);
    let mut new_note_namespace = use_signal(|| Option::<String>::None);
    let mut show_new_namespace = use_signal(|| false);
    let mut new_ns_name = use_signal(|| String::new());
    let nav = use_navigator();
    let auth = use_auth();

    // Load notes from store on mount
    let _loader = use_resource(move || async move {
        let repo = make_repo();
        notes.set(repo.list_notes().await);
        namespaces.set(repo.list_namespaces().await);
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

    let on_navigate_settings = move |_| {
        nav.push(Route::Settings {});
    };

    let handle_create_note =
        move |(name, ns, note_type): (String, Option<String>, String)| {
            let path = if let Some(ref ns) = ns {
                format!("{ns}/{name}")
            } else {
                name
            };
            spawn(async move {
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

    rsx! {
        document::Stylesheet { href: NOTES_CSS }

        div {
            class: "notes-layout",

            Sidebar {
                namespaces: namespaces(),
                notes: notes(),
                active_path: None::<String>,
                user: auth().user,
                on_select_note: on_select_note,
                on_create_note: on_create_note,
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
                } else {
                    div {
                        class: "notes-placeholder",
                        h2 { "Select a note" }
                        p { "Choose a note from the sidebar or create a new one." }
                    }
                }
            }
        }
    }
}
