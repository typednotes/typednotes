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
            class: "p-6",
            h2 { class: "m-0 mb-5 text-lg font-semibold text-neutral-800", "New Note" }

            div {
                class: "mb-4",
                Label { html_for: "new-note-name", "Name" }
                Input {
                    id: "new-note-name",
                    class: "w-full mt-1.5",
                    r#type: "text",
                    placeholder: "my-note",
                    value: name(),
                    oninput: move |evt: FormEvent| name.set(evt.value()),
                }
            }

            div {
                class: "mb-4",
                Label { html_for: "new-note-folder", "Folder" }
                select {
                    id: "new-note-folder",
                    class: "w-full bg-white border border-neutral-300 rounded px-3 py-2 text-sm text-neutral-800 outline-none font-[inherit] mt-1.5 focus:border-primary-500 focus:shadow-[0_0_0_1px_var(--color-primary-500)]",
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
                class: "mb-4",
                Label { html_for: "new-note-type", "Type" }
                select {
                    id: "new-note-type",
                    class: "w-full bg-white border border-neutral-300 rounded px-3 py-2 text-sm text-neutral-800 outline-none font-[inherit] mt-1.5 focus:border-primary-500 focus:shadow-[0_0_0_1px_var(--color-primary-500)]",
                    value: note_type(),
                    onchange: move |evt| note_type.set(evt.value()),
                    option { value: "markdown", "Markdown (.md)" }
                    option { value: "text", "Text (.txt)" }
                }
            }

            div {
                class: "flex gap-2 mt-5",
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
