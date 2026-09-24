use dioxus::prelude::*;

use ui::{Navbar, UserMenu};
use views::{Home, Login, OrgView, ProjectView};

mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(WebNavbar)]
    #[route("/")]
    Home {},
    #[route("/login?:error")]
    Login { error: String },
    #[route("/orgs/:slug?:connected&:error")]
    OrgView { slug: String, connected: String, error: String },
    #[route("/orgs/:slug/projects/:project?:connected&:error")]
    ProjectView { slug: String, project: String, connected: String, error: String },
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    // The server also serves the OAuth round trips (`/auth/...`): plain
    // redirects, merged into the router next to the app and its server
    // functions.
    #[cfg(feature = "server")]
    dioxus::serve(|| async move { Ok(dioxus::server::router(App).merge(api::auth_routes())) });

    #[cfg(not(feature = "server"))]
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
            UserMenu {}
        }

        Outlet::<Route> {}
    }
}
