use dioxus::prelude::*;

use store::{NamespaceInfo, Repository, TypedNoteInfo, TypedNotesConfig};
use ui::{Sidebar, use_auth};

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
pub fn Settings() -> Element {
    let mut notes = use_signal(Vec::<TypedNoteInfo>::new);
    let mut namespaces = use_signal(Vec::<NamespaceInfo>::new);
    let mut notes_root = use_signal(|| String::new());
    let mut save_status = use_signal(|| Option::<&str>::None);
    let nav = use_navigator();
    let auth = use_auth();

    // Load data on mount
    let _loader = use_resource(move || async move {
        let repo = make_repo();
        notes.set(repo.list_notes().await);
        namespaces.set(repo.list_namespaces().await);
        let config = repo.get_config().await;
        notes_root.set(config.notes.root);
    });

    let on_select_note = move |path: String| {
        let encoded = path.replace('/', "~");
        nav.push(Route::NoteDetail {
            note_path: encoded,
        });
    };

    let on_create_note = move |_ns: Option<String>| {
        nav.push(Route::Notes {});
    };

    let on_navigate_settings = move |_| {
        // Already on settings
    };

    let handle_save = move |_| {
        spawn(async move {
            let repo = make_repo();
            let config = TypedNotesConfig::new(notes_root());
            repo.set_config(&config).await;
            save_status.set(Some("success"));
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

                div {
                    class: "settings-content",

                    h1 { "Settings" }

                    div {
                        class: "settings-section",
                        h2 { "Repository Configuration" }

                        div {
                            class: "form-field",
                            label { "Notes root folder" }
                            input {
                                r#type: "text",
                                placeholder: "e.g. notes, docs/notes",
                                value: notes_root(),
                                oninput: move |evt| {
                                    notes_root.set(evt.value());
                                    save_status.set(None);
                                },
                            }
                            p {
                                class: "form-help",
                                "Subfolder within the git repository where notes are stored. Leave empty for root."
                            }
                        }

                        div {
                            class: "form-field",
                            label { "Git remote URL" }
                            input {
                                r#type: "text",
                                placeholder: "Coming soon",
                                disabled: true,
                            }
                            p {
                                class: "form-help",
                                "Remote git repository to sync notes with."
                            }
                        }

                        div {
                            class: "form-actions",
                            button {
                                class: "primary",
                                onclick: handle_save,
                                "Save"
                            }
                            if let Some(status) = save_status() {
                                span {
                                    class: "save-status {status}",
                                    if status == "success" { "Saved" } else { "Error" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
