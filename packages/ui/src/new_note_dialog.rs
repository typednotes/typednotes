use dioxus::prelude::*;
use store::NamespaceInfo;

use crate::components::{Button, ButtonVariant, Input, Label};

/// Inline form for creating a new note.
#[component]
pub fn NewNoteDialog(
    namespaces: Vec<NamespaceInfo>,
    default_namespace: Option<String>,
    on_create: EventHandler<(String, Option<String>, String)>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(|| String::new());
    let mut namespace = use_signal(|| String::new());
    let mut note_type = use_signal(|| "markdown".to_string());

    // Sync default_namespace prop into the signal each time the dialog mounts.
    // use_effect re-runs when the captured prop value changes.
    {
        let init_ns = default_namespace.clone().unwrap_or_default();
        use_effect(move || {
            namespace.set(init_ns.clone());
        });
    }

    let handle_submit = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        let ns = if namespace().is_empty() {
            None
        } else {
            Some(namespace())
        };
        on_create.call((n, ns, note_type()));
    };

    rsx! {
        div {
            class: "modal-body",
            h2 { class: "modal-title", "New Note" }

            div {
                class: "modal-field",
                Label { html_for: "new-note-name", "Name" }
                Input {
                    id: "new-note-name",
                    r#type: "text",
                    placeholder: "my-note",
                    value: name(),
                    oninput: move |evt: FormEvent| name.set(evt.value()),
                }
            }

            div {
                class: "modal-field",
                Label { html_for: "new-note-namespace", "Namespace" }
                select {
                    id: "new-note-namespace",
                    class: "modal-select",
                    value: namespace(),
                    onchange: move |evt| namespace.set(evt.value()),
                    option { value: "", "/ (root)" }
                    for ns in &namespaces {
                        option {
                            key: "{ns.path}",
                            value: "{ns.path}",
                            "{ns.path}"
                        }
                    }
                }
            }

            div {
                class: "modal-field",
                Label { html_for: "new-note-type", "Type" }
                select {
                    id: "new-note-type",
                    class: "modal-select",
                    value: note_type(),
                    onchange: move |evt| note_type.set(evt.value()),
                    option { value: "markdown", "Markdown (.md)" }
                    option { value: "text", "Text (.txt)" }
                }
            }

            div {
                class: "modal-actions",
                Button {
                    variant: ButtonVariant::Primary,
                    onclick: handle_submit,
                    "Create"
                }
                Button {
                    variant: ButtonVariant::Outline,
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}
