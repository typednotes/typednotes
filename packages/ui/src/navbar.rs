use dioxus::prelude::*;

const NAVBAR_CSS: Asset = asset!("/assets/styling/navbar.css");

/// The sticky top bar. `children` are laid out in a row: typically the brand
/// link (give it `class: "navbar-brand"`) and the `UserMenu`, which pushes
/// itself to the right.
#[component]
pub fn Navbar(children: Element) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: NAVBAR_CSS }

        header { id: "navbar",
            nav { class: "navbar-inner", {children} }
        }
    }
}
