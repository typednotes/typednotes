use dioxus::prelude::*;

#[component]
pub fn Navbar(children: Element) -> Element {
    rsx! {
        div {
            class: "flex flex-row px-4 py-3 border-b border-neutral-300 bg-white [&_a]:text-neutral-800 [&_a]:mr-5 [&_a]:no-underline [&_a]:text-sm [&_a]:transition-colors [&_a]:duration-150 [&_a:hover]:text-primary-500 [&_a:hover]:cursor-pointer",
            {children}
        }
    }
}
