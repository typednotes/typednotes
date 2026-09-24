use dioxus::prelude::*;
use ui::{OrgPage, ProjectPage};

/// `/orgs/:slug`, with the OAuth callback's `?connected=` / `?error=`.
#[component]
pub fn OrgView(slug: String, connected: String, error: String) -> Element {
    rsx! { OrgPage { slug, connected, error } }
}

/// `/orgs/:slug/projects/:project`, with the OAuth callback's `?connected=` /
/// `?error=` when a connection was started from the project.
#[component]
pub fn ProjectView(slug: String, project: String, connected: String, error: String) -> Element {
    rsx! { ProjectPage { slug, project, connected, error } }
}
