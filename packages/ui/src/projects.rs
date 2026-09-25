use api::{
    clear_project_repo, current_user, delete_project, get_project, health, list_connections,
    list_projects, list_repos, set_project_repo, Connection, Project, Provider,
};
use dioxus::prelude::*;

use crate::auth::LoginPanel;
use crate::channels::ChannelsSection;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::select::{Select, SelectOption};
use crate::connections::{connect_url, oauth_ready, CONNECTIONS_CSS};
use crate::error_message;
use crate::navigate_to;
use crate::orgs::ORGS_CSS;
use crate::slug_form::{NewSlugForm, Scope};

/// An org's projects, and a form to add one.
#[component]
pub(crate) fn ProjectsPanel(slug: ReadSignal<String>) -> Element {
    let mut projects = use_server_future(move || list_projects(slug()))?;

    rsx! {
        Card {
            CardHeader {
                CardTitle { "Projects" }
                CardDescription { "Each project has a primary repository and its own messaging interfaces." }
            }
            CardContent {
                match projects() {
                    None => rsx! { p { "Loading…" } },
                    Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load projects: {error_message(&e)}" } },
                    Some(Ok(list)) if list.is_empty() => rsx! {
                        p { class: "orgs-empty", "No projects yet — create one below." }
                    },
                    Some(Ok(list)) => rsx! { ProjectTable { org: slug(), projects: list } },
                }
                h4 { class: "projects-new", "New project" }
                NewSlugForm {
                    scope: Scope::Project { org: slug() },
                    name_placeholder: "Website",
                    slug_placeholder: "website",
                    on_created: move |created: String| {
                        projects.restart();
                        navigate_to(&format!("/orgs/{}/projects/{created}", slug()));
                    },
                }
            }
        }
    }
}

#[component]
fn ProjectTable(org: String, projects: Vec<Project>) -> Element {
    rsx! {
        table { class: "orgs-table",
            thead {
                tr {
                    th { "Slug" }
                    th { "Name" }
                    th { "Repository" }
                    th { "Created" }
                }
            }
            tbody {
                for project in projects {
                    tr { key: "{project.id}",
                        td { a { href: "/orgs/{org}/projects/{project.slug}", code { "{project.slug}" } } }
                        td { "{project.name}" }
                        td {
                            match &project.repo {
                                Some(repo) => rsx! { a { href: "{repo.web_url}", target: "_blank", rel: "noopener", "{repo.full_name}" } },
                                None => rsx! { span { class: "orgs-empty", "—" } },
                            }
                        }
                        td { "{project.created_at}" }
                    }
                }
            }
        }
    }
}

/// One project: its primary repository, its interfaces and its inbox.
///
/// `connected` and `error` come from the query string an OAuth round trip
/// started here redirects back with.
#[component]
pub fn ProjectPage(
    slug: ReadSignal<String>,
    project: ReadSignal<String>,
    connected: String,
    error: String,
) -> Element {
    let me = use_server_future(current_user)?;
    let mut detail = use_server_future(move || get_project(slug(), project()))?;

    if let Some(Ok(None)) = me() {
        return rsx! { LoginPanel { error: None } };
    }
    let flash = Provider::from_id(&connected).map(|p| format!("{} connected.", p.name()));

    rsx! {
        document::Link { rel: "stylesheet", href: ORGS_CSS }
        document::Link { rel: "stylesheet", href: CONNECTIONS_CSS }
        div { class: "orgs",
            p { class: "back-link", a { href: "/orgs/{slug}", "← Organisation" } }
            match detail() {
                None => rsx! { p { "Loading…" } },
                Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load this project: {error_message(&e)}" } },
                Some(Ok(d)) => rsx! {
                    Card {
                        CardHeader {
                            CardTitle { "{d.project.name}" }
                            CardDescription {
                                code { "{d.org.slug}/{d.project.slug}" }
                                " · in {d.org.name} · created {d.project.created_at}"
                            }
                        }
                        CardContent {
                            DeleteProject { slug: slug(), project: project() }
                        }
                    }
                    if let Some(message) = flash.clone() {
                        p { class: "orgs-status ok", "{message}" }
                    }
                    if !error.is_empty() {
                        p { class: "orgs-error", "{error}" }
                    }
                    RepoPanel {
                        slug: slug(),
                        project: d.project.clone(),
                        on_changed: move |_| detail.restart(),
                    }
                    ChannelsSection { slug: slug(), project: project() }
                },
            }
        }
    }
}

#[component]
fn DeleteProject(slug: String, project: String) -> Element {
    let mut confirming = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let org = slug.clone();
    rsx! {
        div { class: "conn-actions",
            Button {
                size: ButtonSize::Sm,
                variant: if confirming() { ButtonVariant::Destructive } else { ButtonVariant::Ghost },
                onclick: move |_| {
                    let (slug, project, org) = (slug.clone(), project.clone(), org.clone());
                    async move {
                        if !confirming() {
                            confirming.set(true);
                            return;
                        }
                        match delete_project(slug, project).await {
                            Ok(()) => navigate_to(&format!("/orgs/{org}")),
                            Err(e) => {
                                error.set(Some(error_message(&e)));
                                confirming.set(false);
                            }
                        }
                    }
                },
                if confirming() { "Confirm: delete the project, its interfaces and messages" } else { "Delete project" }
            }
        }
        if let Some(e) = error() {
            p { class: "orgs-error", "{e}" }
        }
    }
}

/// How a code connection reads in a menu: `GitHub @octo`.
fn describe_connection(c: &Connection) -> String {
    format!("{} {}", c.provider.name(), c.label)
}

/// The project's primary repository, and a picker to set it from the
/// repositories a GitHub or GitLab connection of the org can see.
#[component]
fn RepoPanel(slug: String, project: Project, on_changed: EventHandler<()>) -> Element {
    let org = slug.clone();
    let connections = use_server_future(move || list_connections(org.clone()))?;
    let status = use_server_future(health)?;
    let h = status().and_then(|r| r.ok());
    let mut picking = use_signal(|| project.repo.is_none());
    let mut error = use_signal(|| None::<String>);

    let code: Vec<Connection> = match connections() {
        Some(Ok(list)) => list.into_iter().filter(|c| c.provider.is_code()).collect(),
        _ => Vec::new(),
    };

    let clear = {
        let (slug, project_slug) = (slug.clone(), project.slug.clone());
        move |_| {
            let (slug, project_slug) = (slug.clone(), project_slug.clone());
            async move {
                match clear_project_repo(slug, project_slug).await {
                    Ok(_) => {
                        picking.set(true);
                        on_changed.call(());
                    }
                    Err(e) => error.set(Some(error_message(&e))),
                }
            }
        }
    };

    rsx! {
        Card {
            CardHeader {
                CardTitle { "Primary repository" }
                CardDescription { "Read through a GitHub or GitLab connection of the organisation." }
            }
            CardContent {
                if let Some(repo) = project.repo.clone() {
                    div { class: "conn-row",
                        div { class: "conn-main",
                            span { class: "conn-provider", "{repo.provider.name()}" }
                            a { href: "{repo.web_url}", target: "_blank", rel: "noopener", "{repo.full_name}" }
                            if let Some(branch) = repo.default_branch.clone() {
                                span { class: "conn-meta", "default branch " code { "{branch}" } }
                            }
                        }
                        if repo.connection_id.is_none() {
                            div { class: "conn-meta orgs-error", "The connection it was read through was removed." }
                        }
                        div { class: "conn-actions",
                            Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, onclick: move |_| picking.set(!picking()), "Change" }
                            Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: clear, "Clear" }
                        }
                    }
                }
                if picking() {
                    if code.is_empty() {
                        p { class: "conn-meta", "Connect a code host first; you will come back here." }
                        div { class: "conn-oauth",
                            for provider in Provider::CODE {
                                Button {
                                    key: "{provider.id()}",
                                    variant: ButtonVariant::Outline,
                                    disabled: !oauth_ready(h.as_ref(), provider),
                                    onclick: {
                                        let url = connect_url(provider, &slug, Some(&project.slug));
                                        move |_| navigate_to(&url)
                                    },
                                    "Connect {provider.name()}"
                                }
                            }
                        }
                    } else {
                        RepoPicker {
                            slug: slug.clone(),
                            project: project.slug.clone(),
                            connections: code,
                            on_set: move |_| {
                                picking.set(false);
                                on_changed.call(());
                            },
                        }
                    }
                }
                if let Some(e) = error() {
                    p { class: "orgs-error", "{e}" }
                }
            }
        }
    }
}

#[component]
fn RepoPicker(
    slug: String,
    project: String,
    connections: Vec<Connection>,
    on_set: EventHandler<()>,
) -> Element {
    let mut connection = use_signal(|| {
        connections
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_default()
    });
    let mut repo = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let org = slug.clone();
    let repos = use_resource(move || {
        let org = org.clone();
        async move {
            let id = connection();
            if id.is_empty() {
                return Ok(Vec::new());
            }
            list_repos(org, id).await.map_err(|e| error_message(&e))
        }
    });

    let save = move |_| {
        let (slug, project) = (slug.clone(), project.clone());
        async move {
            let Some(full_name) = repo() else {
                error.set(Some("choose a repository".to_string()));
                return;
            };
            busy.set(true);
            match set_project_repo(slug, project, connection(), full_name).await {
                Ok(_) => {
                    error.set(None);
                    on_set.call(());
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };

    rsx! {
        div { class: "conn-subform",
            div { class: "conn-picker",
                div { class: "conn-select",
                    Select::<String> {
                        default_value: connection(),
                        aria_label: "Code connection",
                        on_value_change: move |v: Option<String>| {
                            if let Some(id) = v {
                                repo.set(None);
                                connection.set(id);
                            }
                        },
                        for (i, c) in connections.iter().enumerate() {
                            SelectOption::<String> {
                                key: "{c.id}",
                                index: i,
                                value: c.id.clone(),
                                text_value: describe_connection(c),
                                {describe_connection(c)}
                            }
                        }
                    }
                }
                match repos() {
                    None => rsx! { p { class: "conn-meta", "Loading repositories…" } },
                    Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not list repositories: {e}" } },
                    Some(Ok(list)) if list.is_empty() => rsx! { p { class: "orgs-empty", "This connection sees no repository." } },
                    Some(Ok(list)) => rsx! {
                        div { class: "conn-select conn-select-wide", key: "{connection}",
                            Select::<String> {
                                aria_label: "Repository",
                                on_value_change: move |v: Option<String>| repo.set(v),
                                for (i, r) in list.iter().enumerate() {
                                    SelectOption::<String> {
                                        key: "{r.full_name}",
                                        index: i,
                                        value: r.full_name.clone(),
                                        text_value: r.full_name.clone(),
                                        "{r.full_name}"
                                        if r.private { span { class: "conn-meta", " · private" } }
                                    }
                                }
                            }
                        }
                    },
                }
            }
            Button { disabled: busy() || repo().is_none(), onclick: save, "Set as primary repository" }
            if let Some(e) = error() {
                p { class: "orgs-error", "{e}" }
            }
        }
    }
}
