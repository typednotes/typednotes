use api::{get_workspace, list_connections, list_graphs, list_orgs, list_projects, set_workspace_defaults, UserWorkspace};
use dioxus::prelude::*;
use crate::components::{button::{Button, ButtonVariant}, card::{Card, CardContent, CardDescription, CardHeader, CardTitle}, select::{Select, SelectOption}};
use crate::{auth::LoginPanel, connections::ConnectionsPanel, error_message, notebook::ModelPicker, orgs::ORGS_CSS, projects::RepoPanel, slug_form::{NewSlugForm, Scope}};

pub const SETUP_CSS: Asset = asset!("/assets/styling/setup.css");

#[component]
pub fn WorkspaceLanding() -> Element {
    let workspace = use_server_future(get_workspace)?;
    let nav = navigator();
    use_effect(move || {
        if let Some(Ok(w)) = workspace() {
            if let Some(url) = w.default_url { nav.replace(url); }
            else { nav.replace("/onboarding"); }
        }
    });
    rsx! {
        match workspace() {
            Some(Err(error)) => rsx! { p { class: "orgs-error", "Could not load your default workspace: {error_message(&error)}" } },
            _ => rsx! { p { role: "status", "Opening your workspace…" } },
        }
        Link { to: "/organizations", "Your organizations" }
    }
}

/// Persistent, accessible guidance across the settings pages visited by setup.
#[component]
pub fn WorkspaceGuide() -> Element {
    let workspace = use_resource(get_workspace);
    rsx! {
        document::Stylesheet { href: SETUP_CSS }
        if let Some(Ok(w)) = workspace() {
            if w.setup_required {
                aside { class: "setup-guide", aria_label: "Required workspace setup",
                    strong { "Finish setting up your workspace" }
                    if let Some(requirement) = w.requirements.first() { span { "{requirement}" } }
                    Link { to: "/onboarding", "Continue guided setup" }
                }
            }
        }
    }
}

#[component]
pub(crate) fn WorkspaceTerms() -> Element {
    rsx! {
        div { class: "workspace-terms",
            details { summary { title: "Billing, membership, connections and permission ceilings are shared within an organization.", "Organization" }
                p { "The billing and access boundary: credits, members, roles, connections and notebook permission ceilings belong to the organization." } }
            details { summary { title: "A project groups notebooks, a primary repository and messaging interfaces.", "Project" }
                p { "A workspace inside an organization. Its primary GitHub/GitLab repository holds generated code; its messaging interfaces route messages to this project." } }
            details { summary { title: "A notebook is a computation made of natural-language cells, inputs and outputs.", "Notebook" }
                p { "The basic unit of work: named cells form a reactive computation. Sources provide inputs, nodes compute and sinks publish results. Lode implements its Lean code; Lun builds and runs it." } }
        }
    }
}

#[component]
pub fn OnboardingPage() -> Element {
    let mut workspace = use_server_future(get_workspace)?;
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let nav = navigator();
    let save = move |(org, project, notebook, finish): (String, Option<String>, Option<String>, bool)| async move {
        busy.set(true); error.set(None);
        match set_workspace_defaults(org, project, notebook, finish).await {
            Ok(w) => { workspace.restart(); if finish { if let Some(url) = w.default_url { nav.replace(url); } } },
            Err(e) => error.set(Some(error_message(&e))),
        }
        busy.set(false);
    };
    rsx! {
        document::Stylesheet { href: ORGS_CSS }
        document::Stylesheet { href: SETUP_CSS }
        div { class: "orgs setup",
            header { h2 { "Your default workspace" }
                p { "Choose the organization, project and notebook you want to open after sign-in. Required setup is checked before completion; optional integrations can be added later." } }
            WorkspaceTerms {}
            match workspace() {
                Some(Ok(w)) => rsx! { SetupSteps { workspace: w, busy: busy(), on_save: save, on_refresh: move |_| workspace.restart() } },
                Some(Err(ServerFnError::ServerError { code: 401, .. })) => rsx! { LoginPanel { error: None } },
                Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load setup: {error_message(&e)}" } },
                None => rsx! { p { role: "status", "Loading setup…" } },
            }
            if let Some(error) = error() { p { class: "orgs-error", role: "alert", "{error}" } }
            Link { to: "/organizations", "Manage your organizations" }
        }
    }
}

#[component]
fn SetupSteps(workspace: ReadSignal<UserWorkspace>, busy: bool, on_save: EventHandler<(String, Option<String>, Option<String>, bool)>, on_refresh: EventHandler<()>) -> Element {
    let orgs = use_resource(list_orgs);
    let projects = use_resource(move || async move { match workspace().org { Some(org) => list_projects(org.slug).await, None => Ok(Vec::new()) } });
    let notebooks = use_resource(move || async move { match (workspace().org, workspace().project) { (Some(org), Some(project)) => list_graphs(org.slug, project.slug).await, _ => Ok(Vec::new()) } });
    let connections = use_resource(move || async move { match workspace().org { Some(org) => list_connections(org.slug).await, None => Ok(Vec::new()) } });
    let selected_org = use_memo(move || workspace().org.map(|o| o.slug));
    let selected_project = use_memo(move || workspace().project.map(|p| p.slug));
    let selected_notebook = use_memo(move || workspace().notebook.map(|g| g.slug));
    let w = workspace();
    rsx! {
        Card {
            CardHeader { CardTitle { "1. Organization" } CardDescription { "Choose who owns the billing, members, connections and permission ceilings." } }
            CardContent {
                if let Some(Ok(options)) = orgs() { if !options.is_empty() {
                    Select::<String> { value: Some(selected_org.into()), aria_label: "Default organization", disabled: busy,
                        on_value_change: move |value: Option<String>| { if let Some(value) = value { on_save.call((value, None, None, false)); } },
                        for (index, org) in options.iter().enumerate() { SelectOption::<String> { index, value: org.slug.clone(), text_value: org.name.clone(), "{org.name}" } }
                    }
                } }
                if w.org.is_none() {
                    NewSlugForm { scope: Scope::Org, name_placeholder: "Your organization", slug_placeholder: "your-organization",
                        on_created: move |org: String| on_save.call((org, None, None, false)) }
                }
            }
        }
        if let Some(org) = w.org.clone() {
            Card {
                CardHeader { CardTitle { "2. Project" } CardDescription { "Choose a project for the repository, notebooks and messaging interfaces." } }
                CardContent {
                    if let Some(Ok(options)) = projects() { if !options.is_empty() {
                        Select::<String> { value: Some(selected_project.into()), aria_label: "Default project", disabled: busy,
                            on_value_change: { let org = org.slug.clone(); move |value: Option<String>| { if let Some(value) = value { on_save.call((org.clone(), Some(value), None, false)); } } },
                            for (index, project) in options.iter().enumerate() { SelectOption::<String> { index, value: project.slug.clone(), text_value: project.name.clone(), "{project.name}" } }
                        }
                    } }
                    if w.project.is_none() { NewSlugForm { scope: Scope::Project { org: org.slug.clone() }, name_placeholder: "Your project", slug_placeholder: "your-project",
                        on_created: { let org = org.slug.clone(); move |project: String| on_save.call((org.clone(), Some(project), None, false)) } } }
                }
            }
            if let Some(project) = w.project.clone() {
                Card {
                    CardHeader { CardTitle { "3. Notebook" } CardDescription { "Choose the computation you want to open by default." } }
                    CardContent {
                        if let Some(Ok(options)) = notebooks() { if !options.is_empty() {
                            Select::<String> { value: Some(selected_notebook.into()), aria_label: "Default notebook", disabled: busy,
                                on_value_change: { let (org, project) = (org.slug.clone(), project.slug.clone()); move |value: Option<String>| { if let Some(value) = value { on_save.call((org.clone(), Some(project.clone()), Some(value), false)); } } },
                                for (index, notebook) in options.iter().enumerate() { SelectOption::<String> { index, value: notebook.slug.clone(), text_value: notebook.name.clone(), "{notebook.name}" } }
                            }
                        } }
                        if w.notebook.is_none() { NewSlugForm { scope: Scope::Graph { org: org.slug.clone(), project: project.slug.clone() }, name_placeholder: "Your first notebook", slug_placeholder: "your-notebook",
                            on_created: { let (org, project) = (org.slug.clone(), project.slug.clone()); move |notebook: String| on_save.call((org.clone(), Some(project.clone()), Some(notebook), false)) } } }
                    }
                }
                if w.notebook.is_some() {
                    section { class: "setup-required", aria_label: "Required repository and AI setup",
                        h3 { "4. Repository and AI model" }
                        p { "Connect a code host and a generative AI provider. Choose the repository, allow the intended code-writing scope explicitly, and select a model. Existing connections are reused." }
                        ConnectionsPanel { slug: org.slug.clone(), setup_only: true }
                        RepoPanel { key: "{org.id}-{project.id}", slug: org.slug.clone(), project: project.clone(), on_changed: move |_| on_refresh.call(()) }
                        if let Some(notebook) = w.notebook.clone() {
                            if let Some(Ok(accounts)) = connections() { ModelPicker { key: "{notebook.id}", slug: org.slug.clone(), project: project.slug.clone(), graph: notebook, ai: accounts, on_changed: move |_| on_refresh.call(()) } }
                        }
                        Button { variant: ButtonVariant::Outline, onclick: move |_| on_refresh.call(()), "Refresh setup status" }
                        Link { to: "/orgs/{org.slug}/settings/notebooks", "Review required notebook permissions" }
                    }
                }
                details { summary { title: "Storage, calendars, email and messaging are optional until a notebook uses them.", "Optional integrations" }
                    p { "Add storage, calendars, email or messaging when your notebook needs those resources. They do not block initial setup." }
                    Link { to: "/orgs/{org.slug}/settings/connections", "Other connections" }
                    " · " Link { to: "/orgs/{org.slug}/projects/{project.slug}/settings/interfaces", "Messaging interfaces" }
                }
                Card {
                    CardHeader { CardTitle { "Complete setup" } }
                    CardContent {
                        if !w.requirements.is_empty() { ul { for requirement in w.requirements.iter() { li { "{requirement}" } } } }
                        Button { disabled: busy || !w.requirements.is_empty() || w.notebook.is_none(),
                            onclick: { let (org, project, notebook) = (org.slug.clone(), project.slug.clone(), w.notebook.as_ref().map(|g| g.slug.clone())); move |_| on_save.call((org.clone(), Some(project.clone()), notebook.clone(), true)) },
                            "Save defaults and open notebook" }
                    }
                }
            }
        }
    }
}
