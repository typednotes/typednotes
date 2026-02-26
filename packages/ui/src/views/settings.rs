use dioxus::prelude::*;

use store::TypedNotesConfig;
use crate::components::{Button, ButtonVariant, Input, Label, Textarea, TextareaVariant};
use crate::{ThemeSignal, apply_theme, NoteTree, use_note_tree, use_auth};
use crate::make_repo_for_user;
use crate::Icon;
use crate::icons::{FaCircleHalfStroke, FaSun, FaMoon};

const VIEWS_CSS: Asset = asset!("/src/views/views.css");

/// Shared settings view.
///
/// Platform packages control which sections are visible via props.
#[component]
pub fn SettingsView(
    /// Show the git sync credentials section (web only).
    #[props(default)]
    show_git_sync: bool,
    /// Show the theme selector section.
    #[props(default = true)]
    show_theme: bool,
) -> Element {
    let mut tree = use_note_tree();
    let mut notes_root = use_signal(|| String::new());
    let mut auto_sync_secs = use_signal(|| 300u32);
    let mut save_status = use_signal(|| Option::<&str>::None);

    // Git credentials state (only used when show_git_sync is true)
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

    let auth = use_auth();

    // Load config + optional git credentials on mount
    let _loader = use_resource(move || async move {
        let user_id = auth().user.as_ref().map(|u| u.id.clone());
        let repo = make_repo_for_user(user_id.as_deref());
        let config = repo.get_config().await;
        notes_root.set(config.notes.root);
        auto_sync_secs.set(config.sync.auto_sync_interval_secs);

        if show_git_sync {
            if let Ok(Some(creds)) = api::get_git_credentials().await {
                git_remote_url.set(creds.git_remote_url.unwrap_or_default());
                git_branch.set(creds.git_branch.unwrap_or_else(|| "main".to_string()));
                ssh_public_key.set(creds.ssh_public_key);
            }
        }
    });

    let handle_save = move |_| {
        spawn(async move {
            let user_id = auth().user.as_ref().map(|u| u.id.clone());
            let repo = make_repo_for_user(user_id.as_deref());
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

                    let user_id = auth().user.as_ref().map(|u| u.id.clone());
                    let repo = make_repo_for_user(user_id.as_deref());
                    for file in &remote_files {
                        let ext = file.path.rsplit('.').next().unwrap_or("md");
                        let note_type = store::models::note_type_from_ext(ext);
                        let stem = file.path.trim_end_matches(&format!(".{ext}"));
                        repo.write_note(stem, &file.content, note_type).await;
                    }
                    if !remote_files.is_empty() {
                        tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
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

    rsx! {
        document::Link { rel: "stylesheet", href: VIEWS_CSS }
        div {
            class: "view-page max-w-3xl mx-auto w-full",

            h1 { class: "view-title", "Settings" }

            // Theme section
            if show_theme {
                div {
                    class: "mb-8",
                    h2 { class: "view-section-title", "Theme" }
                    ThemeSelector {}
                }
            }

            // Repository Configuration section
            div {
                class: "mb-8",
                h2 { class: "view-section-title", "Repository Configuration" }

                div {
                    class: "mb-4",
                    Label { html_for: "notes-root", "Notes root folder" }
                    Input {
                        id: "notes-root",
                        class: "w-full mt-1.5",
                        r#type: "text",
                        placeholder: "e.g. notes, docs/notes",
                        value: notes_root(),
                        oninput: move |evt: FormEvent| {
                            notes_root.set(evt.value());
                            save_status.set(None);
                        },
                    }
                    p {
                        class: "view-muted",
                        "Subfolder within the repository where notes are stored. Leave empty for root."
                    }
                }

                div {
                    class: "mb-4",
                    Label { html_for: "auto-sync", if show_git_sync { "Auto-sync interval (seconds)" } else { "Auto-save interval (seconds)" } }
                    Input {
                        id: "auto-sync",
                        class: "w-full mt-1.5",
                        r#type: "number",
                        min: "0",
                        max: "3600",
                        value: "{auto_sync_secs()}",
                        oninput: move |evt: FormEvent| {
                            if let Ok(v) = evt.value().parse::<u32>() {
                                auto_sync_secs.set(v);
                                save_status.set(None);
                            }
                        },
                    }
                    p {
                        class: "view-muted",
                        if show_git_sync {
                            "Automatically save and sync after this many seconds of editing. Set to 0 to disable."
                        } else {
                            "Automatically save after this many seconds of editing. Set to 0 to disable."
                        }
                    }
                }

                div {
                    class: "flex gap-2 mt-5",
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: handle_save,
                        "Save"
                    }
                    if let Some(status) = save_status() {
                        span {
                            class: if status == "success" { "text-[0.8125rem] text-success ml-2" } else { "text-[0.8125rem] text-danger ml-2" },
                            if status == "success" { "Saved" } else { "Error" }
                        }
                    }
                }
            }

            // Git Sync section (web only)
            if show_git_sync {
                div {
                    class: "mb-8",
                    h2 { class: "view-section-title", "Git Sync" }

                    div {
                        class: "mb-4",
                        Label { html_for: "git-remote", "Git remote URL" }
                        Input {
                            id: "git-remote",
                            class: "w-full mt-1.5",
                            r#type: "text",
                            placeholder: "git@github.com:user/repo.git",
                            value: git_remote_url(),
                            oninput: move |evt: FormEvent| {
                                git_remote_url.set(evt.value());
                                git_save_status.set(None);
                            },
                        }
                        p {
                            class: "view-muted",
                            "Remote git repository to sync notes with."
                        }
                    }

                    div {
                        class: "mb-4",
                        Label { html_for: "git-branch", "Git branch" }
                        Input {
                            id: "git-branch",
                            class: "w-full mt-1.5",
                            r#type: "text",
                            placeholder: "main",
                            value: git_branch(),
                            oninput: move |evt: FormEvent| {
                                git_branch.set(evt.value());
                                git_save_status.set(None);
                            },
                        }
                        p {
                            class: "view-muted",
                            "Branch to sync with (e.g. main, master, notes)."
                        }
                    }

                    div {
                        class: "mb-4",
                        Label { html_for: "ssh-key", "SSH Private Key" }
                        Textarea {
                            id: "ssh-key",
                            variant: TextareaVariant::Outline,
                            class: "w-full mt-1.5 font-mono text-[0.8125rem]",
                            placeholder: "-----BEGIN OPENSSH PRIVATE KEY-----\n...\n-----END OPENSSH PRIVATE KEY-----",
                            rows: 8,
                            value: ssh_private_key(),
                            oninput: move |evt: FormEvent| {
                                ssh_private_key.set(evt.value());
                                git_save_status.set(None);
                            },
                        }
                        p {
                            class: "view-muted",
                            if ssh_public_key().is_some() {
                                "A key is already stored. Leave blank to keep it."
                            } else {
                                "Paste your SSH private key. It will be encrypted on the server and never returned."
                            }
                        }
                    }

                    if let Some(pub_key) = ssh_public_key() {
                        div {
                            class: "mb-4",
                            Label { html_for: "ssh-pub-key", "SSH Public Key (add this to your git provider)" }
                            Textarea {
                                id: "ssh-pub-key",
                                variant: TextareaVariant::Outline,
                                class: "w-full mt-1.5 font-mono text-[0.8125rem] settings-pub-key",
                                readonly: true,
                                rows: 3,
                                value: pub_key,
                            }
                        }
                    }

                    div {
                        class: "flex gap-2 mt-5",
                        Button {
                            variant: ButtonVariant::Primary,
                            onclick: handle_git_save,
                            disabled: git_saving(),
                            if git_saving() { "Saving..." } else { "Save Git Settings" }
                        }
                        if let Some(ref status) = git_save_status() {
                            if status == "success" {
                                span {
                                    class: "text-[0.8125rem] text-success ml-2",
                                    "Saved"
                                }
                            } else {
                                span {
                                    class: "text-[0.8125rem] text-danger ml-2",
                                    "{status}"
                                }
                            }
                        }
                    }

                    div {
                        class: "flex gap-2 mt-5",
                        Button {
                            variant: ButtonVariant::Outline,
                            onclick: handle_sync,
                            disabled: is_syncing(),
                            if is_syncing() { "Syncing..." } else { "Sync Now" }
                        }
                        if let Some(ref status) = sync_status() {
                            span {
                                class: if status.starts_with("Error") { "text-[0.8125rem] text-danger ml-2" } else { "text-[0.8125rem] text-success ml-2" },
                                "{status}"
                            }
                        }
                    }

                    // Sync console
                    if !sync_log().is_empty() {
                        {
                            let log_css: Asset = asset!("/src/views/log_panel.css");
                            rsx! {
                                document::Link { rel: "stylesheet", href: log_css }
                                div {
                                    class: "log-panel mt-4 rounded-md max-h-[200px]",
                                    div {
                                        class: "log-panel-header",
                                        span { "Sync Log" }
                                        button {
                                            class: "log-panel-action",
                                            onclick: move |_| sync_log.write().clear(),
                                            "Clear"
                                        }
                                    }
                                    div {
                                        class: "log-panel-body",
                                        for entry in sync_log().iter() {
                                            div { class: "log-panel-entry log-entry-info", "{entry}" }
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

#[component]
fn ThemeSelector() -> Element {
    let mut theme = use_context::<ThemeSignal>();

    let current = theme().unwrap_or_default();
    let is_system = current.is_empty();
    let is_light = current == "light";
    let is_dark = current == "dark";

    let radio_class = |active: bool| {
        if active {
            "theme-card theme-card-active"
        } else {
            "theme-card"
        }
    };

    rsx! {
        div {
            class: "flex flex-wrap gap-3",
            label {
                class: radio_class(is_system),
                onclick: move |_| {
                    apply_theme(None);
                    theme.set(None);
                },
                Icon { icon: FaCircleHalfStroke, width: 14, height: 14 }
                span { "System" }
            }
            label {
                class: radio_class(is_light),
                onclick: move |_| {
                    apply_theme(Some("light"));
                    theme.set(Some("light".to_string()));
                },
                Icon { icon: FaSun, width: 14, height: 14 }
                span { "Light" }
            }
            label {
                class: radio_class(is_dark),
                onclick: move |_| {
                    apply_theme(Some("dark"));
                    theme.set(Some("dark".to_string()));
                },
                Icon { icon: FaMoon, width: 14, height: 14 }
                span { "Dark" }
            }
        }
        p {
            class: "view-muted mt-2",
            "Choose how TypedNotes appears. System follows your OS preference."
        }
    }
}
