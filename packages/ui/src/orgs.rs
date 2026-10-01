use api::{health, list_orgs, Org};
use dioxus::prelude::*;

use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::error_message;
use crate::slug_form::{NewSlugForm, Scope};

pub(crate) const ORGS_CSS: Asset = asset!("/assets/styling/orgs.css");

/// The signed-in home page: the caller's orgs, and a form to add one.
///
/// Both reads use `use_server_future`, so the server renders the list and
/// the status line into the initial HTML and the client hydrates the same
/// data — no hydration mismatch, and no loading flash on first paint.
#[component]
pub fn OrgsPanel() -> Element {
    let mut orgs = use_server_future(list_orgs)?;
    let status = use_server_future(health)?;

    rsx! {
        document::Link { rel: "stylesheet", href: ORGS_CSS }

        div { class: "orgs",
            StatusLine { health: status().and_then(|r| r.ok()) }

            NewOrgForm { on_created: move |_| orgs.restart() }

            Card {
                CardHeader {
                    CardTitle { "Your organisations" }
                    CardDescription { "Orgs you belong to, newest first. Connections live in each org." }
                }
                CardContent {
                    match orgs() {
                        Some(Ok(list)) if list.is_empty() => rsx! {
                            p { class: "orgs-empty", "No organisations yet — create one above." }
                        },
                        Some(Ok(list)) => rsx! { OrgTable { orgs: list } },
                        Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load orgs: {error_message(&e)}" } },
                        None => rsx! { p { "Loading…" } },
                    }
                }
            }
        }
    }
}

/// What this deployment is missing, so a fresh deploy says so (no database,
/// migrations not applied, vault or broker not configured) instead of
/// failing opaquely later.
#[component]
fn StatusLine(health: Option<api::Health>) -> Element {
    let Some(h) = health else {
        return rsx! { p { class: "orgs-status warn", "status unknown" } };
    };
    let mut missing = Vec::new();
    if !h.database {
        missing.push("database unreachable");
    } else if !h.schema {
        missing.push("core schema not applied");
    }
    if h.database && !h.ledger {
        missing.push("credits schema absent (no credits)");
    }
    if !h.vault {
        missing.push("vault not configured or refusing the app (no connections)");
    }
    if !h.liaison {
        missing.push("credential broker not configured (no tests)");
    }
    if h.database && h.schema && !h.computations {
        missing.push("computations schema not applied (no notebooks)");
    }
    if !h.lode {
        missing.push("code writing not configured (notebooks cannot be implemented)");
    }
    if !h.lun {
        missing.push("code runtime not configured (notebooks cannot run)");
    }
    if !h.compute {
        missing.push("notebook database not configured (no database sinks)");
    }
    let class = if !h.database || !h.schema {
        "err"
    } else if missing.is_empty() {
        "ok"
    } else {
        "warn"
    };
    let text = if missing.is_empty() {
        "every service is configured".to_string()
    } else {
        missing.join(" · ")
    };
    rsx! { p { class: "orgs-status {class}", "{text}" } }
}

#[component]
fn OrgTable(orgs: Vec<Org>) -> Element {
    rsx! {
        table { class: "orgs-table",
            thead {
                tr {
                    th { "Slug" }
                    th { "Name" }
                    th { "Role" }
                    th { "Created" }
                }
            }
            tbody {
                for org in orgs {
                    tr { key: "{org.id}",
                        // A plain link: the router lives in each app crate,
                        // and every app routes `/orgs/:slug` to `OrgPage`.
                        td { Link { to: "/orgs/{org.slug}", code { "{org.slug}" } } }
                        td { "{org.name}" }
                        td { "{org.role}" }
                        td { "{org.created_at}" }
                    }
                }
            }
        }
    }
}

/// Create an org: its slug is checked while typing, so a taken one is
/// refused before submitting.
#[component]
fn NewOrgForm(on_created: EventHandler<String>) -> Element {
    rsx! {
        Card {
            CardHeader {
                CardTitle { "New organisation" }
                CardDescription { "You become its owner, and it starts with welcome credits." }
            }
            CardContent {
                NewSlugForm {
                    scope: Scope::Org,
                    name_placeholder: "Acme Labs",
                    slug_placeholder: "acme-labs",
                    on_created,
                }
            }
        }
    }
}
