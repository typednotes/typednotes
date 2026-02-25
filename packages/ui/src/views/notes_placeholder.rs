use dioxus::prelude::*;

/// Empty state shown when no note is selected.
#[component]
pub fn NotesPlaceholder() -> Element {
    rsx! {
        div {
            class: "flex-1 flex flex-col items-center justify-center text-neutral-600",
            h2 { class: "m-0 mb-2 font-normal text-neutral-800 text-lg", "Select a note" }
            p { class: "m-0 text-sm text-neutral-600", "Choose a note from the sidebar or create a new one." }
        }
    }
}
