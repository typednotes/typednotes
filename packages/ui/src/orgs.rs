use api::{create_org, health, list_orgs, validate_org, Org};
use dioxus::prelude::*;

use crate::components::button::Button;
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;

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
                        Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load orgs: {e}" } },
                        None => rsx! { p { "Loading…" } },
                    }
                }
            }
        }
    }
}

/// What this deployment is missing, so a fresh deploy says so (no database,
/// migrations not applied, vault or liaison not configured) instead of
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
        missing.push("ledger schema absent (no credits)");
    }
    if !h.vault {
        missing.push("vault not configured (no connections)");
    }
    if !h.liaison {
        missing.push("liaison not configured (no tests)");
    }
    let class = if !h.database || !h.schema {
        "err"
    } else if missing.is_empty() {
        "ok"
    } else {
        "warn"
    };
    let text = if missing.is_empty() {
        "database, ledger, vault and liaison all configured".to_string()
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
                        td { a { href: "/orgs/{org.slug}", code { "{org.slug}" } } }
                        td { "{org.name}" }
                        td { "{org.role}" }
                        td { "{org.created_at}" }
                    }
                }
            }
        }
    }
}

/// Create an org. Validation runs here first (same rule as the server, from
/// `api::validate_org`) so a malformed slug is explained without a round
/// trip; the server still enforces it, and uniqueness is the database's.
#[component]
fn NewOrgForm(on_created: EventHandler<Org>) -> Element {
    let mut slug = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move |evt: FormEvent| async move {
        evt.prevent_default();
        let (s, n) = (slug().trim().to_lowercase(), name().trim().to_string());
        if let Err(message) = validate_org(&s, &n) {
            error.set(Some(message));
            return;
        }
        busy.set(true);
        match create_org(s, n).await {
            Ok(org) => {
                slug.set(String::new());
                name.set(String::new());
                error.set(None);
                on_created.call(org);
            }
            Err(e) => error.set(Some(e.to_string())),
        }
        busy.set(false);
    };

    rsx! {
        Card {
            CardHeader {
                CardTitle { "New organisation" }
                CardDescription { "You become its owner, and it starts with welcome credits." }
            }
            CardContent {
                form { class: "orgs-form", onsubmit: submit,
                    div { class: "orgs-field",
                        Label { html_for: "org-slug", "Slug" }
                        Input {
                            id: "org-slug",
                            placeholder: "acme-labs",
                            value: slug(),
                            oninput: move |evt: FormEvent| slug.set(evt.value()),
                        }
                    }
                    div { class: "orgs-field",
                        Label { html_for: "org-name", "Name" }
                        Input {
                            id: "org-name",
                            placeholder: "Acme Labs",
                            value: name(),
                            oninput: move |evt: FormEvent| name.set(evt.value()),
                        }
                    }
                    Button { r#type: "submit", disabled: busy(), "Create" }
                }
                if let Some(message) = error() {
                    p { class: "orgs-error", "{message}" }
                }
            }
        }
    }
}
