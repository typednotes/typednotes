use dioxus::prelude::*;

use store::{NamespaceInfo, TypedNoteInfo};
use ui::components::{use_toast, ToastOptions};
use ui::{NoteEditor, LogLevel, log_activity, use_activity_log};

use super::make_repo;
use crate::Route;

#[component]
pub fn NoteDetail(note_path: String) -> Element {
    // Track decoded path in a signal so use_resource re-runs on route param change
    let mut path_signal = use_signal(|| note_path.replace('~', "/"));
    if *path_signal.peek() != note_path.replace('~', "/") {
        path_signal.set(note_path.replace('~', "/"));
    }

    let mut notes = use_context::<Signal<Vec<TypedNoteInfo>>>();
    let mut namespaces = use_context::<Signal<Vec<NamespaceInfo>>>();
    let mut current_note = use_signal(|| Option::<TypedNoteInfo>::None);
    let mut auto_sync_secs = use_signal(|| 300u32);
    let nav = use_navigator();
    let mut activity_log = use_activity_log();
    let toast_api = use_toast();

    // Load current note and refresh from remote on path change
    let _loader = use_resource(move || {
        let path = path_signal();
        async move {
            let repo = make_repo();
            current_note.set(repo.get_note(&path).await);
            let config = repo.get_config().await;
            auto_sync_secs.set(config.sync.auto_sync_interval_secs);

            // Background pull to refresh note from remote
            spawn(async move {
                log_activity(&mut activity_log, LogLevel::Info, &format!("Pulling latest for {path}..."));
                match api::pull_notes().await {
                    Ok(remote_files) => {
                        let repo = make_repo();
                        for file in &remote_files {
                            let ext = file.path.rsplit('.').next().unwrap_or("md");
                            let note_type = store::models::note_type_from_ext(ext);
                            let stem = file.path.trim_end_matches(&format!(".{ext}"));
                            repo.write_note(stem, &file.content, note_type).await;
                        }
                        if !remote_files.is_empty() {
                            current_note.set(repo.get_note(&path).await);
                            notes.set(repo.list_notes().await);
                            namespaces.set(repo.list_namespaces().await);
                        }
                        log_activity(&mut activity_log, LogLevel::Success, &format!("Pulled {} notes", remote_files.len()));
                    }
                    Err(e) => {
                        log_activity(&mut activity_log, LogLevel::Warning, &format!("Pull: {e}"));
                    }
                }
            });
        }
    });

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
                toast_api.success("Saved".to_string(), ToastOptions::new());

                // Git sync
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
        });
    };

    let handle_rename = move |new_name: String| {
        let old_path = path_signal();
        spawn(async move {
            if let Some(note) = current_note() {
                let ext = store::models::ext_from_note_type(&note.r#type);
                // Compute new path: replace filename stem, keep namespace and extension
                let new_path = if let Some(ns) = &note.namespace {
                    format!("{ns}/{new_name}.{ext}")
                } else {
                    format!("{new_name}.{ext}")
                };

                if new_path == old_path {
                    return;
                }

                let repo = make_repo();
                repo.rename_note(&old_path, &new_path).await;
                current_note.set(repo.get_note(&new_path).await);
                notes.set(repo.list_notes().await);
                namespaces.set(repo.list_namespaces().await);

                // Update path signal and URL
                path_signal.set(new_path.clone());
                let encoded = new_path.replace('/', "~");
                nav.replace(Route::NoteDetail { note_path: encoded });

                log_activity(&mut activity_log, LogLevel::Info, &format!("Renamed to {new_path}"));
                toast_api.success("Renamed".to_string(), ToastOptions::new());
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

    rsx! {
        if let Some(note) = current_note() {
            NoteEditor {
                key: "{note.sha}",
                note: note.clone(),
                on_save: handle_save,
                on_delete: handle_delete,
                on_rename: handle_rename,
                auto_sync_interval_secs: auto_sync_secs(),
            }
        } else {
            div {
                class: "flex-1 flex flex-col items-center justify-center text-neutral-600",
                h2 { class: "m-0 mb-2 font-normal text-neutral-800 text-lg", "Loading..." }
            }
        }
    }
}
