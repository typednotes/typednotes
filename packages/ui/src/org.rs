use api::{current_user, get_org, Provider};
use dioxus::prelude::*;

use crate::auth::LoginPanel;
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::connections::ConnectionsPanel;
use crate::error_message;
use crate::orgs::ORGS_CSS;
use crate::projects::ProjectsPanel;

/// One org: its credits (from `ledger`), its projects and its connections.
///
/// `connected` and `error` come from the query string the OAuth callback
/// redirects back with (`?connected=github`, `?error=…`); empty when absent.
#[component]
pub fn OrgPage(slug: ReadSignal<String>, connected: String, error: String) -> Element {
    // Every hook runs before any branch, so hook order is stable.
    let me = use_server_future(current_user)?;
    let detail = use_server_future(move || get_org(slug()))?;

    if let Some(Ok(None)) = me() {
        return rsx! { LoginPanel { error: None } };
    }

    let flash = Provider::from_id(&connected).map(|p| format!("{} connected.", p.name()));

    rsx! {
        document::Link { rel: "stylesheet", href: ORGS_CSS }
        div { class: "orgs",
            p { class: "back-link", a { href: "/", "← Your organisations" } }
            match detail() {
                None => rsx! { p { "Loading…" } },
                Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load this organisation: {error_message(&e)}" } },
                Some(Ok(d)) => rsx! {
                    Card {
                        CardHeader {
                            CardTitle { "{d.org.name}" }
                            CardDescription {
                                code { "{d.org.slug}" }
                                " · you are {d.org.role} · created {d.org.created_at}"
                            }
                        }
                        CardContent {
                            match d.credits {
                                Some(credits) => rsx! { p { class: "org-credits", "{credits} credits available" } },
                                None => rsx! { p { class: "orgs-empty", "Credits unavailable: ledger's schema is not applied." } },
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
                    ConnectionsPanel { slug: d.org.slug.clone() }
                },
            }
        }
    }
}
