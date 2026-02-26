use dioxus::prelude::*;

const VIEWS_CSS: Asset = asset!("/src/views/views.css");

/// A full-screen overlay that centers its children in a modal card.
/// Clicking outside the card triggers `on_close`.
#[component]
pub fn ModalOverlay(on_close: EventHandler<()>, children: Element) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: VIEWS_CSS }
        div {
            class: "modal-overlay",
            onclick: move |_| on_close.call(()),
            div {
                class: "modal-card",
                onclick: move |evt: Event<MouseData>| evt.stop_propagation(),
                {children}
            }
        }
    }
}
