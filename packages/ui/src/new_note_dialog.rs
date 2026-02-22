use dioxus::prelude::*;
use store::NamespaceInfo;

/// Inline form for creating a new note.
#[component]
pub fn NewNoteDialog(
    namespaces: Vec<NamespaceInfo>,
    default_namespace: Option<String>,
    on_create: EventHandler<(String, Option<String>, String)>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(|| String::new());
    let mut namespace = use_signal(move || default_namespace.unwrap_or_default());
    let mut note_type = use_signal(|| "markdown".to_string());

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
            class: "new-note-form",
            h2 { "New Note" }

            div {
                class: "form-field",
                label { "Name" }
                input {
                    r#type: "text",
                    placeholder: "my-note",
                    value: name(),
                    oninput: move |evt| name.set(evt.value()),
                }
            }

            div {
                class: "form-field",
                label { "Folder" }
                select {
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
                class: "form-field",
                label { "Type" }
                select {
                    value: note_type(),
                    onchange: move |evt| note_type.set(evt.value()),
                    option { value: "markdown", "Markdown (.md)" }
                    option { value: "text", "Text (.txt)" }
                }
            }

            div {
                class: "form-actions",
                button {
                    class: "primary",
                    onclick: handle_submit,
                    "Create"
                }
                button {
                    class: "secondary",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}
