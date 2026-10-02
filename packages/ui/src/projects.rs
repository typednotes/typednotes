use api::{
    clear_project_repo, current_user, delete_project, get_project, health, list_connections,
    list_projects, list_repo_page, repository_name, set_project_repo, Connection, Project, Provider, Repo,
};
use dioxus::prelude::*;

use crate::auth::LoginPanel;
use crate::channels::InboxPanel;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::select::{use_selected, Select, SelectOption};
use crate::components::{input::Input, label::Label};
use crate::connections::{connect_url, oauth_ready, CONNECTIONS_CSS};
use crate::error_message;
use crate::navigate_to;
use crate::notebook::GraphsPanel;
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
                        td { Link { to: "/orgs/{org}/projects/{project.slug}", code { "{project.slug}" } } }
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

/// One project, for the work in it: its notebooks and its inbox. What it
/// is set up with — its repository, its interfaces, its name — is in its
/// settings, which the header links to (and to the right section when
/// something is missing).
///
/// `connected` and `error` come from older OAuth redirects; flows started
/// from the project's settings come back there.
#[component]
pub fn ProjectPage(
    slug: ReadSignal<String>,
    project: ReadSignal<String>,
    connected: String,
    error: String,
) -> Element {
    let me = use_server_future(current_user)?;
    let detail = use_server_future(move || get_project(slug(), project()))?;

    if let Some(Ok(None)) = me() {
        return rsx! { LoginPanel { error: None } };
    }
    let flash = Provider::from_id(&connected).map(|p| format!("{} connected.", p.name()));

    rsx! {
        document::Link { rel: "stylesheet", href: ORGS_CSS }
        document::Link { rel: "stylesheet", href: CONNECTIONS_CSS }
        div { class: "orgs",
            p { class: "back-link", Link { to: "/orgs/{slug}", "← Organisation" } }
            match detail() {
                None => rsx! { p { "Loading…" } },
                Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load this project: {error_message(&e)}" } },
                Some(Ok(d)) => {
                    let settings = format!("/orgs/{}/projects/{}/settings", d.org.slug, d.project.slug);
                    rsx! {
                        Card {
                            CardHeader {
                                CardTitle { title: "Project: a primary code repository, its notebooks and messaging interfaces.", "{d.project.name}" }
                                CardDescription {
                                    "in {d.org.name} · "
                                    match &d.project.repo {
                                        Some(r) => rsx! { a { href: "{r.web_url}", target: "_blank", rel: "noopener", "{r.full_name}" } },
                                        None => rsx! { "no repository" },
                                    }
                                }
                            }
                            CardContent {
                                div { class: "workspace-links",
                                    Link { to: "{settings}", "Settings" }
                                    Link { to: "{settings}/repository", "Repository" }
                                    Link { to: "{settings}/interfaces", "Interfaces" }
                                }
                                if d.project.repo.is_none() {
                                    div { class: "setup-hints",
                                        p {
                                            "Notebooks need a primary repository, where their code is written: "
                                            Link { to: "{settings}/repository", "choose one" }
                                            "."
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(message) = flash.clone() {
                            p { class: "orgs-status ok", "{message}" }
                        }
                        if !error.is_empty() {
                            p { class: "orgs-error", "{error}" }
                        }
                        GraphsPanel { slug: slug(), project: project() }
                        InboxPanel { slug: slug(), project: project() }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn DeleteProject(slug: String, project: String) -> Element {
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
pub(crate) fn RepoPanel(slug: String, project: Project, on_changed: EventHandler<()>) -> Element {
    let org = slug.clone();
    let connections = use_resource(move || list_connections(org.clone()));
    let status = use_resource(health);
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
                    if connections().is_none() {
                        p { class: "conn-meta", role: "status", "Loading code connections…" }
                    } else if let Some(Err(error)) = connections() {
                        p { class: "orgs-error", "Could not load code connections: {error_message(&error)}" }
                    } else if code.is_empty() {
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
                            key: "{slug}-{project.id}",
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
    connections: ReadSignal<Vec<Connection>>,
    on_set: EventHandler<()>,
) -> Element {
    let mut connection = use_signal(|| {
        connections()
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_default()
    });
    let mut owner = use_signal(String::new);
    let mut repo = use_signal(String::new);
    let mut filter = use_signal(String::new);
    let mut page = use_signal(|| 1_u32);
    let mut previous = use_signal(Vec::<Repo>::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let selected_connection = use_selected(connection.into());
    let selected_owner = use_selected(owner.into());
    let org = slug.clone();
    let permissions_href = format!("/orgs/{slug}/settings/notebooks");
    let mut pages = use_resource(move || {
        let org = org.clone();
        let id = connection();
        let requested_page = page();
        async move {
            let result = if id.is_empty() { Err("Select a code connection.".into()) }
                else { list_repo_page(org, id.clone(), requested_page).await.map_err(|e| error_message(&e)) };
            (id, requested_page, result)
        }
    });
    let current = pages().filter(|(id, number, _)| *id == connection() && *number == page()).map(|(_, _, result)| result);
    let mut inventory = previous();
    if let Some(Ok(current)) = &current { inventory.extend(current.repos.clone()); }
    inventory.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    inventory.dedup_by(|a, b| a.full_name == b.full_name);
    let owners: Vec<String> = inventory.iter().filter_map(|r| r.full_name.split_once('/').map(|(owner, _)| owner.to_string()))
        .collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let choices: Vec<Repo> = inventory.iter().filter(|r| owner().is_empty() || r.full_name.split_once('/').is_some_and(|(name, _)| name == owner()))
        .filter(|r| r.full_name.to_lowercase().contains(&filter().to_lowercase())).cloned().collect();
    use_effect(move || {
        let id = connection();
        if !id.is_empty() && !connections().iter().any(|c| c.id == id) {
            connection.set(String::new()); repo.set(String::new()); owner.set(String::new());
            previous.set(Vec::new()); page.set(1);
        }
    });
    let provider = connections().iter().find(|c| c.id == connection()).map(|c| c.provider);
    let parsed = provider.and_then(|provider| repository_name(provider, &repo()).ok());
    let displayed_repository = use_memo(move || {
        connections().iter().find(|c| c.id == connection()).and_then(|c| repository_name(c.provider, &repo()).ok())
    });
    let loading = pages.state()() == UseResourceState::Pending;
    let can_load = current.as_ref().and_then(|result| result.as_ref().ok()).and_then(|result| result.next_page);
    let choice_count = choices.len();

    let save = move |event: FormEvent| {
        event.prevent_default();
        let (slug, project) = (slug.clone(), project.clone());
        let selected_connection = connection();
        let selected_repo = repo();
        async move {
            busy.set(true);
            error.set(None);
            match set_project_repo(slug, project, selected_connection, selected_repo).await {
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
        form { class: "conn-subform repo-picker", onsubmit: save,
            div { class: "conn-grid",
                div { class: "orgs-field conn-select",
                    Label { html_for: "repo-connection", "Code connection" }
                    Select::<String> {
                        id: "repo-connection", value: Some(selected_connection), disabled: busy(),
                        aria_label: "Code connection",
                        on_value_change: move |v: Option<String>| {
                            if let Some(id) = v {
                                if id != connection() {
                                    repo.set(String::new()); owner.set(String::new()); filter.set(String::new());
                                    previous.set(Vec::new()); page.set(1); error.set(None); connection.set(id);
                                }
                            }
                        },
                        for (i, c) in connections().iter().enumerate() {
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
                div { class: "orgs-field conn-select",
                    Label { html_for: "repo-owner", "Account or organization" }
                    Select::<String> {
                        id: "repo-owner", value: Some(selected_owner), aria_label: "Account or organization",
                        disabled: busy() || owners.is_empty() || provider.is_none(),
                        on_value_change: move |value: Option<String>| { if let Some(value) = value { owner.set(value); repo.set(String::new()); } },
                        SelectOption::<String> { index: 0usize, value: String::new(), text_value: "All accounts and organizations", "All accounts and organizations" }
                        for (index, name) in owners.iter().enumerate() {
                            SelectOption::<String> { key: "{name}", index: index + 1, value: name.clone(), text_value: name.clone(), "{name}" }
                        }
                    }
                }
            }
            div { class: "orgs-field",
                Label { html_for: "repo-filter", "Filter loaded repositories" }
                Input { id: "repo-filter", value: filter(), disabled: busy(), placeholder: "Search by owner or repository name",
                    oninput: move |event: FormEvent| filter.set(event.value()) }
            }
            div { class: "orgs-field conn-select",
                Label { html_for: "repo-menu", "Repository" }
                Select::<String> {
                    id: "repo-menu", key: "{connection}", value: Some(displayed_repository.into()), aria_label: "Repository",
                    disabled: busy() || choice_count == 0 || provider.is_none(),
                    on_value_change: move |value: Option<String>| { if let Some(value) = value { repo.set(value); error.set(None); } },
                    for (index, r) in choices.iter().enumerate() {
                        SelectOption::<String> { key: "{r.full_name}", index, value: r.full_name.clone(), text_value: r.full_name.clone(), aria_label: r.full_name.clone(),
                            "{r.full_name}" if r.private { span { class: "conn-meta", " · private" } } }
                    }
                }
            }
            if loading { p { class: "conn-meta", role: "status", "Loading repositories…" } }
            if let Some(Err(message)) = &current {
                p { class: "orgs-error", "Could not list repositories: {message} ", Link { to: permissions_href.clone(), "Review notebook permissions" } }
                Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Outline, onclick: move |_| pages.restart(), "Retry repository inventory" }
            }
            if !loading && current.as_ref().is_some_and(Result::is_ok) && choices.is_empty() {
                p { class: "conn-meta", "No loaded repository matches. Load another page or enter the repository name below." }
            }
            div { class: "conn-actions",
                p { class: "conn-meta", "{inventory.len()} repositories loaded" }
                if let Some(next) = can_load {
                    Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Outline, disabled: loading || busy(), onclick: {
                        let inventory = inventory.clone(); move |_| { previous.set(inventory.clone()); page.set(next); }
                    }, "Load more repositories" }
                }
            }
            if current.as_ref().is_some_and(|r| r.as_ref().is_ok_and(|r| r.truncated)) {
                p { class: "conn-meta", "Inventory is limited to 10,000 repositories. Enter a specific repository below." }
            }
            div { class: "orgs-field",
                Label { html_for: "repo-name", "Repository name or URL" }
                Input { id: "repo-name", value: repo(), disabled: busy(), placeholder: "owner/repository or https://github.com/owner/repository",
                    oninput: move |event: FormEvent| { repo.set(event.value()); owner.set(String::new()); error.set(None); } }
                p { class: "conn-meta", "Choose from the menus or type a repository directly. Saving verifies this exact repository through the connection's read grant; browsing uses its inventory grant." }
            }
            Button { r#type: "submit", disabled: busy() || parsed.is_none(),
                if busy() { "Checking repository…" } else { "Set as primary repository" } }
            if let Some(e) = error() {
                p { class: "orgs-error", "{e}" }
            }
        }
    }
}
