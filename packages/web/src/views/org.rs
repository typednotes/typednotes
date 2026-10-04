use dioxus::prelude::*;
use ui::{AccountPage, OrgPage, OrgSettingsPage, ProjectPage, ProjectSettingsPage};

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

/// `/orgs/:slug/projects/:project/graphs/:graph`: a notebook.
#[component]
pub fn GraphView(slug: String, project: String, graph: String) -> Element {
    let nav=use_navigator();
    use_effect(move ||{nav.replace(format!("/orgs/{slug}/projects/{project}"));});
    let _=graph;
    rsx!{p{role:"status","Opening project notes…"}}
}

/// `/settings`: your account.
#[component]
pub fn AccountView() -> Element {
    rsx! { AccountPage {} }
}

/// `/orgs/:slug/settings`: the org's settings, at their first section.
#[component]
pub fn OrgSettingsHome(slug: String) -> Element {
    rsx! { OrgSettingsPage { slug, section: String::new(), connected: String::new(), error: String::new() } }
}

/// `/orgs/:slug/settings/:section`, with an OAuth round trip's
/// `?connected=` / `?error=`.
#[component]
pub fn OrgSettingsView(slug: String, section: String, connected: String, error: String) -> Element {
    rsx! { OrgSettingsPage { slug, section, connected, error } }
}

/// `/orgs/:slug/projects/:project/settings`: at their first section.
#[component]
pub fn ProjectSettingsHome(slug: String, project: String) -> Element {
    rsx! { ProjectSettingsPage { slug, project, section: String::new(), connected: String::new(), error: String::new() } }
}

/// `/orgs/:slug/projects/:project/settings/:section`.
#[component]
pub fn ProjectSettingsView(
    slug: String,
    project: String,
    section: String,
    connected: String,
    error: String,
) -> Element {
    rsx! { ProjectSettingsPage { slug, project, section, connected, error } }
}
