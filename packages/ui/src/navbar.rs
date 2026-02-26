use dioxus::prelude::*;

const VIEWS_CSS: Asset = asset!("/src/views/views.css");

#[component]
pub fn Navbar(children: Element) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: VIEWS_CSS }
        div {
            class: "navbar",
            {children}
        }
    }
}
