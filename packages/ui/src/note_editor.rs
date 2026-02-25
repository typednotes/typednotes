use dioxus::prelude::*;
use store::TypedNoteInfo;
use crate::components::{Button, ButtonVariant, Input, Textarea, TextareaVariant};
use crate::Icon;
use crate::icons::FaTrashCan;

#[component]
pub fn NoteEditor(
    note: TypedNoteInfo,
    on_save: EventHandler<String>,
    on_delete: EventHandler<()>,
    #[props(default)] on_rename: EventHandler<String>,
    #[props(default = 300)] auto_sync_interval_secs: u32,
) -> Element {
    let mut content = use_signal({
        let initial = note.note.clone();
        move || initial
    });
    let mut title = use_signal({
        let initial = note.name.clone();
        move || initial
    });
    let mut dirty = use_signal(|| false);

    let handle_blur = move |_| {
        if dirty() {
            on_save.call(content());
            dirty.set(false);
        }
    };

    let handle_title_blur = move |_| {
        let new_name = title().trim().to_string();
        if !new_name.is_empty() && new_name != note.name {
            on_rename.call(new_name);
        }
    };

    // Save on unmount if dirty
    use_drop(move || {
        if dirty() {
            on_save.call(content());
        }
    });

    // Auto-sync timer
    #[cfg(target_arch = "wasm32")]
    {
        let interval = auto_sync_interval_secs;
        use_effect(move || {
            if interval == 0 {
                return;
            }
            spawn(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(interval * 1000).await;
                    if dirty() {
                        on_save.call(content());
                        dirty.set(false);
                    }
                }
            });
        });
    }

    rsx! {
        div {
            class: "flex flex-col h-full w-full px-6 py-6",

            // Title row: editable name + unsaved indicator + delete
            div {
                class: "flex items-start justify-between gap-4 mb-4",
                Input {
                    class: "flex-1 text-2xl font-bold border-none bg-transparent p-0 shadow-none focus:ring-0",
                    r#type: "text",
                    value: title(),
                    oninput: move |evt: FormEvent| title.set(evt.value()),
                    onblur: handle_title_blur,
                }
                div {
                    class: "flex items-center gap-2 shrink-0 pt-1",
                    if dirty() {
                        span {
                            class: "text-[0.6875rem] text-neutral-600 italic",
                            "Unsaved"
                        }
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: "text-neutral-400 hover:text-danger p-1",
                        title: "Delete note",
                        onclick: move |_| on_delete.call(()),
                        Icon { icon: FaTrashCan, width: 14, height: 14 }
                    }
                }
            }

            // Content area — fills remaining space; parent pane scrolls
            Textarea {
                variant: TextareaVariant::Ghost,
                class: "flex-1 w-full text-neutral-800 dark:text-neutral-200 p-0 font-sans text-base leading-[1.7] resize-none placeholder:text-neutral-400",
                value: content(),
                placeholder: "Start writing...",
                oninput: move |evt: FormEvent| {
                    content.set(evt.value());
                    dirty.set(true);
                },
                onblur: handle_blur,
            }
        }
    }
}
