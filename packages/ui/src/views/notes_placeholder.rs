use dioxus::prelude::*;

const VIEWS_CSS: Asset = asset!("/src/views/views.css");

/// Empty state shown when no note is selected.
#[component]
pub fn NotesPlaceholder() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: VIEWS_CSS }
        div {
            class: "view-placeholder",
            h2 { "Select a note" }
            p { "Choose a note from the sidebar or create a new one." }
        }
    }
}
