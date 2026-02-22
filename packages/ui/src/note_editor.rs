use dioxus::prelude::*;
use store::TypedNoteInfo;

const EDITOR_CSS: Asset = asset!("/assets/styling/note_editor.css");

#[component]
pub fn NoteEditor(
    note: TypedNoteInfo,
    breadcrumb: Option<String>,
    on_save: EventHandler<String>,
    on_delete: EventHandler<()>,
) -> Element {
    let mut content = use_signal({
        let initial = note.note.clone();
        move || initial
    });
    let mut dirty = use_signal(|| false);

    let handle_blur = move |_| {
        if dirty() {
            on_save.call(content());
            dirty.set(false);
        }
    };

    rsx! {
        document::Stylesheet { href: EDITOR_CSS }

        div {
            class: "note-editor",

            // Breadcrumb bar
            div {
                class: "note-editor-breadcrumb",
                div {
                    class: "note-editor-breadcrumb-path",
                    if let Some(ref bc) = breadcrumb {
                        span { "{bc}" }
                        span { " / " }
                    }
                    span { "{note.name}" }
                }
                div {
                    class: "note-editor-actions",
                    if dirty() {
                        span {
                            class: "unsaved-indicator",
                            "Unsaved changes"
                        }
                    }
                    button {
                        class: "danger",
                        onclick: move |_| on_delete.call(()),
                        "Delete"
                    }
                }
            }

            // Content area
            div {
                class: "note-editor-body",
                div {
                    class: "note-editor-content",
                    h1 {
                        class: "note-editor-title",
                        "{note.name}"
                    }
                    textarea {
                        value: content(),
                        placeholder: "Start writing...",
                        oninput: move |evt| {
                            content.set(evt.value());
                            dirty.set(true);
                        },
                        onblur: handle_blur,
                    }
                }
            }
        }
    }
}
