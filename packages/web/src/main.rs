use dioxus::prelude::*;

use ui::Navbar;
use views::Home;

mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(WebNavbar)]
    #[route("/")]
    Home {},
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        // Global app resources
        document::Stylesheet { href: ui::COMPONENTS_THEME }
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        Router::<Route> {}
    }
}

/// A web-specific Router around the shared `Navbar` component
/// which allows us to use the web-specific `Route` enum.
#[component]
fn WebNavbar() -> Element {
    rsx! {
        Navbar {
            Link {
                to: Route::Home {},
                "Typednotes"
            }
        }

        Outlet::<Route> {}
    }
}
