use dioxus::prelude::*;

use ui::{Navbar, UserMenu};
use views::{
    AccountView, GraphView, Home, Login, OrgSettingsHome, OrgSettingsView, OrgView,
    ProjectSettingsHome, ProjectSettingsView, ProjectView,
};

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
    #[route("/settings")]
    AccountView {},
    #[route("/orgs/:slug/settings")]
    OrgSettingsHome { slug: String },
    #[route("/orgs/:slug/settings/:section?:connected&:error")]
    OrgSettingsView { slug: String, section: String, connected: String, error: String },
    #[route("/orgs/:slug/projects/:project?:connected&:error")]
    ProjectView { slug: String, project: String, connected: String, error: String },
    #[route("/orgs/:slug/projects/:project/settings")]
    ProjectSettingsHome { slug: String, project: String },
    #[route("/orgs/:slug/projects/:project/settings/:section?:connected&:error")]
    ProjectSettingsView { slug: String, project: String, section: String, connected: String, error: String },
    #[route("/orgs/:slug/projects/:project/graphs/:graph")]
    GraphView { slug: String, project: String, graph: String },
}

const FAVICON: Asset = ui::BRAND_LOGO;
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    // The server also serves the OAuth round trips (`/auth/...`): plain
    // redirects, merged into the router next to the app and its server
    // functions.
    // And it runs the notebooks' source scheduler (docs/computations.md §3.1).
    #[cfg(feature = "server")]
    dioxus::serve(|| async move {
        api::start_background();
        Ok(dioxus::server::router(App).merge(api::auth_routes()))
    });

    #[cfg(not(feature = "server"))]
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        // Global app resources
        document::Title { "Typednotes" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Stylesheet { href: ui::COMPONENTS_THEME }
        document::Stylesheet { href: ui::APP_THEME }
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
                class: "navbar-brand",
                to: Route::Home {},
                ui::Logo {}
                "Typednotes"
            }
            UserMenu {}
        }

        main { class: "page", Outlet::<Route> {} }
    }
}
