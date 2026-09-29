use api::{current_user, get_org, list_connections, Provider};
use dioxus::prelude::*;

use crate::auth::LoginPanel;
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::error_message;
use crate::orgs::ORGS_CSS;
use crate::projects::ProjectsPanel;

/// One org, for the work in it: its projects. What it is set up with —
/// members, connections, notebook settings — is in its settings, which the
/// header links to, with a hint when a notebook would miss something.
///
/// `connected` and `error` come from older OAuth redirects (flows now come
/// back to the settings they started from) and from a connect flow that
/// could not start.
#[component]
pub fn OrgPage(slug: ReadSignal<String>, connected: String, error: String) -> Element {
    // Every hook runs before any branch, so hook order is stable.
    let me = use_server_future(current_user)?;
    let detail = use_server_future(move || get_org(slug()))?;
    let connections = use_server_future(move || list_connections(slug()))?;

    if let Some(Ok(None)) = me() {
        return rsx! { LoginPanel { error: None } };
    }

    let flash = Provider::from_id(&connected).map(|p| format!("{} connected.", p.name()));
    let (has_ai, has_code) = match connections() {
        Some(Ok(list)) => (
            list.iter().any(|c| c.provider.is_ai()),
            list.iter().any(|c| c.provider.is_code()),
        ),
        // Unknown: no hint rather than a wrong one.
        _ => (true, true),
    };

    rsx! {
        document::Link { rel: "stylesheet", href: ORGS_CSS }
        div { class: "orgs",
            p { class: "back-link", a { href: "/", "← Your organisations" } }
            match detail() {
                None => rsx! { p { "Loading…" } },
                Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load this organisation: {error_message(&e)}" } },
                Some(Ok(d)) => {
                    let settings = format!("/orgs/{}/settings", d.org.slug);
                    rsx! {
                        Card {
                            CardHeader {
                                CardTitle { "{d.org.name}" }
                                CardDescription {
                                    "you are {d.org.role} · "
                                    match d.credits {
                                        Some(credits) => rsx! { "{credits} credits available" },
                                        None => rsx! { "credits unavailable" },
                                    }
                                }
                            }
                            CardContent {
                                div { class: "workspace-links",
                                    a { href: "{settings}", "Settings" }
                                    a { href: "{settings}/members", "Members" }
                                    a { href: "{settings}/connections", "Connections" }
                                }
                                if !has_ai || !has_code {
                                    div { class: "setup-hints",
                                        if !has_code {
                                            p {
                                                "Connect GitHub or GitLab so projects can have a repository: "
                                                a { href: "{settings}/connections", "connections" }
                                                "."
                                            }
                                        }
                                        if !has_ai {
                                            p {
                                                "Connect an AI provider so notebooks can be implemented: "
                                                a { href: "{settings}/connections", "connections" }
                                                "."
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(message) = flash {
                            p { class: "orgs-status ok", "{message}" }
                        }
                        if !error.is_empty() {
                            p { class: "orgs-error", "{error}" }
                        }
                        ProjectsPanel { slug: d.org.slug.clone() }
                    }
                }
            }
        }
    }
}
