use dioxus::prelude::*;

use store::TypedNotesConfig;
use ui::components::{Button, ButtonVariant, Input, Label};

use super::make_repo;

#[component]
pub fn Settings() -> Element {
    let mut notes_root = use_signal(|| String::new());
    let mut auto_sync_secs = use_signal(|| 30u32);
    let mut save_status = use_signal(|| Option::<&str>::None);

    let _loader = use_resource(move || async move {
        let repo = make_repo();
        let config = repo.get_config().await;
        notes_root.set(config.notes.root);
        auto_sync_secs.set(config.sync.auto_sync_interval_secs);
    });

    let handle_save = move |_| {
        spawn(async move {
            let repo = make_repo();
            let config = TypedNotesConfig::new(notes_root()).with_sync_interval(auto_sync_secs());
            repo.set_config(&config).await;
            save_status.set(Some("success"));
        });
    };

    rsx! {
        div {
            class: "max-w-3xl mx-auto w-full px-6 py-8",

            h1 { class: "text-[2rem] font-bold text-neutral-800 m-0 mb-8", "Settings" }

            div {
                class: "mb-8",
                h2 { class: "text-lg font-semibold text-neutral-800 m-0 mb-4 pb-2 border-b border-neutral-300", "Repository Configuration" }

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
                        class: "text-xs text-neutral-600 mt-1",
                        "Subfolder within the repository where notes are stored. Leave empty for root."
                    }
                }

                div {
                    class: "mb-4",
                    Label { html_for: "auto-sync", "Auto-save interval (seconds)" }
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
                        class: "text-xs text-neutral-600 mt-1",
                        "Automatically save after this many seconds of editing. Set to 0 to disable."
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
        }
    }
}
