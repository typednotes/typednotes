use dioxus::prelude::*;
use crate::components::{input::Input, label::Label};

/// Native checkbox semantics with the shared dx input and label components.
#[component]
pub(crate) fn PermissionCheck(id: String, label: String, checked: bool, disabled: bool, on_change: EventHandler<bool>) -> Element {
    rsx! {
        div { class: "permission-check",
            Input { id: id.clone(), r#type: "checkbox", checked, disabled,
                oninput: move |_: FormEvent| on_change.call(!checked) }
            Label { html_for: id, "{label}" }
        }
    }
}
