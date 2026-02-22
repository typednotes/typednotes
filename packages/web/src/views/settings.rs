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

    // Git credentials state
    let mut git_remote_url = use_signal(String::new);
    let mut ssh_private_key = use_signal(String::new);
    let mut ssh_public_key = use_signal(|| Option::<String>::None);
    let mut git_save_status = use_signal(|| Option::<String>::None);
    let mut git_saving = use_signal(|| false);

    // Load data on mount
    let _loader = use_resource(move || async move {
        let repo = make_repo();
        notes.set(repo.list_notes().await);
        namespaces.set(repo.list_namespaces().await);
        let config = repo.get_config().await;
        notes_root.set(config.notes.root);

        // Load git credentials
        if let Ok(Some(creds)) = api::get_git_credentials().await {
            git_remote_url.set(creds.git_remote_url.unwrap_or_default());
            ssh_public_key.set(creds.ssh_public_key);
        }
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

    let on_create_namespace = move |_parent: Option<String>| {
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

    let handle_git_save = move |_| {
        spawn(async move {
            git_save_status.set(None);
            git_saving.set(true);

            let key = if ssh_private_key().trim().is_empty() {
                None
            } else {
                Some(ssh_private_key())
            };

            match api::save_git_credentials(git_remote_url(), key).await {
                Ok(creds) => {
                    ssh_public_key.set(creds.ssh_public_key);
                    git_remote_url.set(creds.git_remote_url.unwrap_or_default());
                    ssh_private_key.set(String::new());
                    git_save_status.set(Some("success".to_string()));
                }
                Err(e) => {
                    git_save_status.set(Some(e.to_string()));
                }
            }
            git_saving.set(false);
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
                on_create_namespace: on_create_namespace,
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

                    div {
                        class: "settings-section",
                        h2 { "Git Sync" }

                        div {
                            class: "form-field",
                            label { "Git remote URL" }
                            input {
                                r#type: "text",
                                placeholder: "git@github.com:user/repo.git",
                                value: git_remote_url(),
                                oninput: move |evt| {
                                    git_remote_url.set(evt.value());
                                    git_save_status.set(None);
                                },
                            }
                            p {
                                class: "form-help",
                                "Remote git repository to sync notes with."
                            }
                        }

                        div {
                            class: "form-field",
                            label { "SSH Private Key" }
                            textarea {
                                class: "ssh-key-textarea",
                                placeholder: "-----BEGIN OPENSSH PRIVATE KEY-----\n...\n-----END OPENSSH PRIVATE KEY-----",
                                rows: 8,
                                value: ssh_private_key(),
                                oninput: move |evt| {
                                    ssh_private_key.set(evt.value());
                                    git_save_status.set(None);
                                },
                            }
                            p {
                                class: "form-help",
                                if ssh_public_key().is_some() {
                                    "A key is already stored. Leave blank to keep it."
                                } else {
                                    "Paste your SSH private key. It will be encrypted on the server and never returned."
                                }
                            }
                        }

                        if let Some(pub_key) = ssh_public_key() {
                            div {
                                class: "form-field",
                                label { "SSH Public Key (add this to your git provider)" }
                                textarea {
                                    class: "ssh-key-textarea",
                                    readonly: true,
                                    rows: 3,
                                    value: pub_key,
                                }
                            }
                        }

                        div {
                            class: "form-actions",
                            button {
                                class: "primary",
                                onclick: handle_git_save,
                                disabled: git_saving(),
                                if git_saving() { "Saving..." } else { "Save Git Settings" }
                            }
                            if let Some(ref status) = git_save_status() {
                                if status == "success" {
                                    span {
                                        class: "save-status success",
                                        "Saved"
                                    }
                                } else {
                                    span {
                                        class: "save-status error",
                                        "{status}"
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
