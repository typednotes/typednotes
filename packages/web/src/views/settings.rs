use dioxus::prelude::*;

use store::{NamespaceInfo, Repository, TypedNoteInfo, TypedNotesConfig};
use ui::{Sidebar, use_auth};

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
pub fn Settings() -> Element {
    let mut notes = use_signal(Vec::<TypedNoteInfo>::new);
    let mut namespaces = use_signal(Vec::<NamespaceInfo>::new);
    let mut notes_root = use_signal(|| String::new());
    let mut auto_sync_secs = use_signal(|| 30u32);
    let mut save_status = use_signal(|| Option::<&str>::None);
    let nav = use_navigator();
    let auth = use_auth();

    // Git credentials state
    let mut git_remote_url = use_signal(String::new);
    let mut git_branch = use_signal(|| "main".to_string());
    let mut ssh_private_key = use_signal(String::new);
    let mut ssh_public_key = use_signal(|| Option::<String>::None);
    let mut git_save_status = use_signal(|| Option::<String>::None);
    let mut git_saving = use_signal(|| false);

    // Sync state
    let mut sync_status = use_signal(|| Option::<String>::None);
    let mut is_syncing = use_signal(|| false);
    let mut sync_log = use_signal(Vec::<String>::new);

    // Load data on mount
    let _loader = use_resource(move || async move {
        let repo = make_repo();
        notes.set(repo.list_notes().await);
        namespaces.set(repo.list_namespaces().await);
        let config = repo.get_config().await;
        notes_root.set(config.notes.root);
        auto_sync_secs.set(config.sync.auto_sync_interval_secs);

        // Load git credentials
        if let Ok(Some(creds)) = api::get_git_credentials().await {
            git_remote_url.set(creds.git_remote_url.unwrap_or_default());
            git_branch.set(creds.git_branch.unwrap_or_else(|| "main".to_string()));
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
            let config = TypedNotesConfig::new(notes_root()).with_sync_interval(auto_sync_secs());
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

            match api::save_git_credentials(git_remote_url(), key, Some(git_branch())).await {
                Ok(creds) => {
                    ssh_public_key.set(creds.ssh_public_key);
                    git_remote_url.set(creds.git_remote_url.unwrap_or_default());
                    git_branch.set(creds.git_branch.unwrap_or_else(|| "main".to_string()));
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

    let handle_sync = move |_| {
        spawn(async move {
            sync_status.set(None);
            is_syncing.set(true);
            sync_log.write().push(format!("[{}] Starting sync...", current_time()));

            sync_log.write().push(format!("[{}] Pulling from remote...", current_time()));
            match api::pull_notes().await {
                Ok(remote_files) => {
                    let count = remote_files.len();
                    sync_log.write().push(format!("[{}] Received {count} files from remote", current_time()));

                    let repo = make_repo();
                    for file in &remote_files {
                        let ext = file.path.rsplit('.').next().unwrap_or("md");
                        let note_type = store::models::note_type_from_ext(ext);
                        let stem = file.path.trim_end_matches(&format!(".{ext}"));
                        repo.write_note(stem, &file.content, note_type).await;
                    }
                    if !remote_files.is_empty() {
                        notes.set(repo.list_notes().await);
                        namespaces.set(repo.list_namespaces().await);
                    }
                    sync_log.write().push(format!("[{}] Sync complete: {count} notes imported", current_time()));
                    sync_status.set(Some(format!("Synced {count} notes")));
                }
                Err(e) => {
                    sync_log.write().push(format!("[{}] ERROR: {e}", current_time()));
                    sync_status.set(Some(format!("Error: {e}")));
                }
            }
            is_syncing.set(false);
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
                    active_path: None::<String>,
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
                            label { "Auto-sync interval (seconds)" }
                            input {
                                r#type: "number",
                                min: "0",
                                max: "3600",
                                value: "{auto_sync_secs()}",
                                oninput: move |evt| {
                                    if let Ok(v) = evt.value().parse::<u32>() {
                                        auto_sync_secs.set(v);
                                        save_status.set(None);
                                    }
                                },
                            }
                            p {
                                class: "form-help",
                                "Automatically save and sync after this many seconds of editing. Set to 0 to disable."
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
                            label { "Git branch" }
                            input {
                                r#type: "text",
                                placeholder: "main",
                                value: git_branch(),
                                oninput: move |evt| {
                                    git_branch.set(evt.value());
                                    git_save_status.set(None);
                                },
                            }
                            p {
                                class: "form-help",
                                "Branch to sync with (e.g. main, master, notes)."
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

                        div {
                            class: "form-actions",
                            button {
                                class: "secondary",
                                onclick: handle_sync,
                                disabled: is_syncing(),
                                if is_syncing() { "Syncing..." } else { "Sync Now" }
                            }
                            if let Some(ref status) = sync_status() {
                                span {
                                    class: if status.starts_with("Error") { "save-status error" } else { "save-status success" },
                                    "{status}"
                                }
                            }
                        }

                        // Sync console
                        if !sync_log().is_empty() {
                            div {
                                class: "sync-console",
                                div {
                                    class: "sync-console-header",
                                    span { "Sync Log" }
                                    button {
                                        onclick: move |_| sync_log.write().clear(),
                                        "Clear"
                                    }
                                }
                                div {
                                    class: "sync-console-entries",
                                    for entry in sync_log().iter() {
                                        div { "{entry}" }
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

#[cfg(target_arch = "wasm32")]
fn current_time() -> String {
    let date = js_sys::Date::new_0();
    let h = date.get_hours();
    let m = date.get_minutes();
    let s = date.get_seconds();
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(not(target_arch = "wasm32"))]
fn current_time() -> String {
    "00:00:00".to_string()
}
