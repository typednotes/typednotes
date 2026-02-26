use dioxus::prelude::*;

use crate::components::{use_toast, ToastOptions};
use crate::{NoteEditor, NoteTree, use_note_tree, LogLevel, log_activity, use_activity_log, use_auth};
use crate::make_repo_for_user;

const VIEWS_CSS: Asset = asset!("/src/views/views.css");

/// Shared note detail view.
///
/// Loads, edits, saves, renames, and deletes a note. Platform packages provide
/// navigation callbacks and feature flags for git integration.
#[component]
pub fn NoteDetailView(
    /// URL-decoded note path (e.g. "folder/note.md").
    note_path: String,
    /// Called after deleting a note — navigate back to the notes list.
    on_navigate_notes: EventHandler<()>,
    /// Called after renaming a note — navigate to the new path.
    on_navigate_note: EventHandler<String>,
    /// Whether to sync saves/deletes to the git remote (web only).
    #[props(default)]
    enable_git_sync: bool,
    /// Whether to pull from remote on load (web only).
    #[props(default)]
    enable_pull_on_load: bool,
    /// Whether rename is enabled.
    #[props(default = true)]
    enable_rename: bool,
) -> Element {
    // Track decoded path in a signal so use_resource re-runs on route param change
    let mut path_signal = use_signal(|| note_path.clone());
    if *path_signal.peek() != note_path {
        path_signal.set(note_path.clone());
    }

    let mut tree = use_note_tree();
    let mut current_note = use_signal(|| Option::<store::TypedNoteInfo>::None);
    let mut auto_sync_secs = use_signal(|| 300u32);
    let mut activity_log = use_activity_log();
    let toast_api = use_toast();
    let auth = use_auth();

    // Load current note and optionally refresh from remote
    let _loader = use_resource(move || {
        let path = path_signal();
        async move {
            let user_id = auth().user.as_ref().map(|u| u.id.clone());
            let repo = make_repo_for_user(user_id.as_deref());
            current_note.set(repo.get_note(&path).await);
            let config = repo.get_config().await;
            auto_sync_secs.set(config.sync.auto_sync_interval_secs);

            if enable_pull_on_load {
                spawn(async move {
                    log_activity(&mut activity_log, LogLevel::Info, &format!("Pulling latest for {path}..."));
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
                                current_note.set(repo.get_note(&path).await);
                                let user_id = auth().user.as_ref().map(|u| u.id.clone());
                                tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
                            }
                            log_activity(&mut activity_log, LogLevel::Success, &format!("Pulled {} notes", remote_files.len()));
                        }
                        Err(e) => {
                            log_activity(&mut activity_log, LogLevel::Warning, &format!("Pull: {e}"));
                        }
                    }
                });
            }
        }
    });

    let handle_save = move |content: String| {
        let path = path_signal();
        spawn(async move {
            let user_id = auth().user.as_ref().map(|u| u.id.clone());
            let repo = make_repo_for_user(user_id.as_deref());
            if let Some(note) = current_note() {
                let stem = path.trim_end_matches(&format!(
                    ".{}",
                    store::models::ext_from_note_type(&note.r#type)
                ));
                repo.write_note(stem, &content, &note.r#type).await;
                current_note.set(repo.get_note(&path).await);
                tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
                log_activity(&mut activity_log, LogLevel::Info, &format!("Saved {path}"));
                toast_api.success("Saved".to_string(), ToastOptions::new());

                // Git sync (if enabled)
                if enable_git_sync {
                    match api::sync_note(path.clone(), content.clone(), note.r#type.clone()).await {
                        Ok(()) => {
                            log_activity(&mut activity_log, LogLevel::Success, &format!("Synced {path}"));
                            toast_api.success("Synced".to_string(), ToastOptions::new());
                        }
                        Err(e) => {
                            log_activity(&mut activity_log, LogLevel::Error, &format!("Sync error: {e}"));
                            toast_api.error(format!("Sync failed: {e}"), ToastOptions::new());
                            #[cfg(target_arch = "wasm32")]
                            web_sys::console::warn_1(&format!("Git sync: {e}").into());
                        }
                    }
                }
            }
        });
    };

    let handle_rename = move |new_name: String| {
        let old_path = path_signal();
        spawn(async move {
            if let Some(note) = current_note() {
                let ext = store::models::ext_from_note_type(&note.r#type);
                let new_path = if let Some(ns) = &note.namespace {
                    format!("{ns}/{new_name}.{ext}")
                } else {
                    format!("{new_name}.{ext}")
                };

                if new_path == old_path {
                    return;
                }

                let user_id = auth().user.as_ref().map(|u| u.id.clone());
                let repo = make_repo_for_user(user_id.as_deref());
                repo.rename_note(&old_path, &new_path).await;
                current_note.set(repo.get_note(&new_path).await);
                tree.set(NoteTree::refresh_for(user_id.as_deref()).await);

                path_signal.set(new_path.clone());
                log_activity(&mut activity_log, LogLevel::Info, &format!("Renamed to {new_path}"));
                toast_api.success("Renamed".to_string(), ToastOptions::new());

                on_navigate_note.call(new_path);
            }
        });
    };

    let handle_delete = move |_| {
        let path = path_signal();
        spawn(async move {
            let user_id = auth().user.as_ref().map(|u| u.id.clone());
            let repo = make_repo_for_user(user_id.as_deref());
            repo.delete_note(&path).await;
            tree.set(NoteTree::refresh_for(user_id.as_deref()).await);
            log_activity(&mut activity_log, LogLevel::Info, &format!("Deleted {path}"));

            if enable_git_sync {
                match api::delete_note_remote(path.clone()).await {
                    Ok(()) => log_activity(&mut activity_log, LogLevel::Success, &format!("Deleted remote {path}")),
                    Err(e) => {
                        log_activity(&mut activity_log, LogLevel::Error, &format!("Delete sync error: {e}"));
                        #[cfg(target_arch = "wasm32")]
                        web_sys::console::warn_1(&format!("Git delete sync: {e}").into());
                    }
                }
            }

            on_navigate_notes.call(());
        });
    };

    rsx! {
        if let Some(note) = current_note() {
            if enable_rename {
                NoteEditor {
                    key: "{note.sha}",
                    note: note.clone(),
                    on_save: handle_save,
                    on_delete: handle_delete,
                    on_rename: handle_rename,
                    auto_sync_interval_secs: auto_sync_secs(),
                }
            } else {
                NoteEditor {
                    key: "{note.sha}",
                    note: note.clone(),
                    on_save: handle_save,
                    on_delete: handle_delete,
                    auto_sync_interval_secs: auto_sync_secs(),
                }
            }
        } else {
            document::Link { rel: "stylesheet", href: VIEWS_CSS }
            div {
                class: "view-placeholder",
                h2 { "Loading..." }
            }
        }
    }
}
