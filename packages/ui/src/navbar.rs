use dioxus::prelude::*;

const NAVBAR_CSS: Asset = asset!("/assets/styling/navbar.css");

/// The full canonical shadowed logo, scaled inside the app's colored pill.
pub const BRAND_LOGO: Asset = asset!("/assets/logo/logo.svg");

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

/// Decorative: the brand link carries the name. SVG alpha blends its complete
/// mathematical shadow into the pill on either background.
#[component]
pub fn Logo() -> Element {
    rsx! {
        span { class: "navbar-logo-pill",
            img { class: "navbar-logo", src: BRAND_LOGO, alt: "", width: "32", height: "32" }
        }
    }
}
