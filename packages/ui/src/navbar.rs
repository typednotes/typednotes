use dioxus::prelude::*;

const NAVBAR_CSS: Asset = asset!("/assets/styling/navbar.css");

/// The logo's small variant (`scripts/logo.py`), sized for the navbar: one
/// for light pages, one for dark.
const MARK_LIGHT: Asset = asset!("/assets/logo/mark-light.svg");
const MARK_DARK: Asset = asset!("/assets/logo/mark-dark.svg");

/// The sticky top bar. `children` are laid out in a row: typically the brand
/// link (give it `class: "navbar-brand"`, with a `Logo` in it) and the
/// `UserMenu`, which pushes itself to the right.
#[component]
pub fn Navbar(children: Element) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: NAVBAR_CSS }

        header { id: "navbar",
            nav { class: "navbar-inner", {children} }
        }
    }
}

/// The Typednotes mark, in the version for the current colour scheme. Both
/// are in the page and CSS shows one, so it follows the scheme as the rest of
/// the theme does, with no script. Decorative: the brand link carries the name.
#[component]
pub fn Logo() -> Element {
    rsx! {
        img { class: "navbar-logo navbar-logo-light", src: MARK_LIGHT, alt: "", width: "28", height: "28" }
        img { class: "navbar-logo navbar-logo-dark", src: MARK_DARK, alt: "", width: "28", height: "28" }
    }
}
