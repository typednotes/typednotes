use dioxus::prelude::*;

use store::{NamespaceInfo, TypedNoteInfo};
use ui::components::{use_toast, ToastOptions};
use ui::{NoteEditor, LogLevel, log_activity, use_activity_log};

use super::make_repo;
use crate::Route;

#[component]
pub fn NoteDetail(note_path: String) -> Element {
    let mut path_signal = use_signal(|| note_path.replace('~', "/"));
    if *path_signal.peek() != note_path.replace('~', "/") {
        path_signal.set(note_path.replace('~', "/"));
    }

    let mut notes = use_context::<Signal<Vec<TypedNoteInfo>>>();
    let mut namespaces = use_context::<Signal<Vec<NamespaceInfo>>>();
    let mut current_note = use_signal(|| Option::<TypedNoteInfo>::None);
    let mut auto_sync_secs = use_signal(|| 30u32);
    let nav = use_navigator();
    let mut activity_log = use_activity_log();
    let toast_api = use_toast();

    let _loader = use_resource(move || {
        let path = path_signal();
        async move {
            let repo = make_repo();
            current_note.set(repo.get_note(&path).await);
            let config = repo.get_config().await;
            auto_sync_secs.set(config.sync.auto_sync_interval_secs);
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
            }
        });
    };

    let handle_delete = move |_| {
        let path = path_signal();
        spawn(async move {
            let repo = make_repo();
            repo.delete_note(&path).await;
            log_activity(&mut activity_log, LogLevel::Info, &format!("Deleted {path}"));
            notes.set(repo.list_notes().await);
            namespaces.set(repo.list_namespaces().await);
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
