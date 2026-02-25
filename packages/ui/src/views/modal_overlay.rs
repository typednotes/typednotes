use dioxus::prelude::*;

/// A full-screen overlay that centers its children in a modal card.
/// Clicking outside the card triggers `on_close`.
#[component]
pub fn ModalOverlay(on_close: EventHandler<()>, children: Element) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 flex items-center justify-center bg-black/30",
            style: "z-index: 2000",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-white dark:bg-neutral-800 rounded-lg shadow-lg max-w-md w-full mx-4",
                onclick: move |evt: Event<MouseData>| evt.stop_propagation(),
                {children}
            }
        }
    }
}
