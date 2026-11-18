use dioxus::prelude::*;
use std::fmt;
use std::str::FromStr;

use ui::{Navbar, UserMenu};
use views::{
    AccountView, GraphView, Home, Login, Onboarding, OrgSettingsHome, OrgSettingsView, OrgView,
    Organizations, ProjectSettingsHome, ProjectSettingsView, ProjectView,
};

mod views;

/// An opaque URL path segment, already percent-decoded by Dioxus.
#[derive(Debug, Clone, PartialEq)]
struct PathSegment(String);

impl FromStr for PathSegment {
    // Dioxus 0.7.10 generates an error-constructor call even for infallible
    // parsers. Rust 1.100 flags that call when the error type is `Infallible`.
    type Err = String;

    fn from_str(segment: &str) -> Result<Self, Self::Err> {
        Ok(Self(segment.to_owned()))
    }
}

impl fmt::Display for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<PathSegment> for String {
    fn from(segment: PathSegment) -> Self {
        segment.0
    }
}

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(WebNavbar)]
    #[route("/")]
    Home {},
    #[route("/organizations")]
    Organizations {},
    #[route("/onboarding")]
    Onboarding {},
    #[route("/s/:token")]
    SharedView { token: PathSegment },
    #[route("/login?:error")]
    Login { error: String },
    #[route("/orgs/:slug?:connected&:error")]
    OrgView { slug: PathSegment, connected: String, error: String },
    #[route("/settings")]
    AccountView {},
    #[route("/orgs/:slug/settings")]
    OrgSettingsHome { slug: PathSegment },
    #[route("/orgs/:slug/settings/:section?:connected&:error")]
    OrgSettingsView { slug: PathSegment, section: PathSegment, connected: String, error: String },
    #[route("/orgs/:slug/projects/:project?:connected&:error")]
    ProjectView { slug: PathSegment, project: PathSegment, connected: String, error: String },
    #[route("/orgs/:slug/projects/:project/settings")]
    ProjectSettingsHome { slug: PathSegment, project: PathSegment },
    #[route("/orgs/:slug/projects/:project/settings/:section?:connected&:error")]
    ProjectSettingsView { slug: PathSegment, project: PathSegment, section: PathSegment, connected: String, error: String },
    #[route("/orgs/:slug/projects/:project/graphs/:graph")]
    GraphView { slug: PathSegment, project: PathSegment, graph: PathSegment },
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
        document::Title { "Typednotes β" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Stylesheet { href: ui::COMPONENTS_THEME }
        document::Stylesheet { href: ui::APP_THEME }
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        Router::<Route> {}
    }
}

#[component]
fn SharedView(#[props(into)] token: String) -> Element {
    rsx! { ui::SharedNotebookPage { token } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_path_segments_round_trip_as_opaque_text() {
        let route: Route = "/orgs/team%20one/projects/notes%25/settings/connectors"
            .parse()
            .unwrap();
        assert_eq!(
            route,
            Route::ProjectSettingsView {
                slug: PathSegment("team one".into()),
                project: PathSegment("notes%".into()),
                section: PathSegment("connectors".into()),
                connected: String::new(),
                error: String::new(),
            }
        );
        assert_eq!(route.to_string().parse::<Route>().unwrap(), route);
    }
}

/// A web-specific Router around the shared `Navbar` component
/// which allows us to use the web-specific `Route` enum.
#[component]
fn WebNavbar() -> Element {
    let current_path = use_route::<Route>().to_string();
    rsx! {
        Navbar {
            Link {
                class: "navbar-brand",
                to: Route::Home {},
                ui::Logo {}
                span {
                    "Typednotes"
                    sup { class: "navbar-beta", title: "Beta", aria_label: "Beta", "β" }
                }
            }
            UserMenu {}
        }

        main { class: "page", ui::WorkspaceGuide {current_path} Outlet::<Route> {} }
    }
}
