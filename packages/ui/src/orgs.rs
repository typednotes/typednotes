use api::{create_org, health, list_orgs, validate_org, Org};
use dioxus::prelude::*;

use crate::components::button::Button;
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;

const ORGS_CSS: Asset = asset!("/assets/styling/orgs.css");

/// The whole app, for now: which orgs exist, and a form to add one.
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
                    CardTitle { "Organisations" }
                    CardDescription { "Every org in the core schema, newest first." }
                }
                CardContent {
                    match orgs() {
                        Some(Ok(list)) if list.is_empty() => rsx! {
                            p { class: "orgs-empty", "No organisations yet." }
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

/// Database and schema reachability, so a fresh deploy says what is missing
/// (no database, or migrations not applied yet) instead of failing opaquely.
#[component]
fn StatusLine(health: Option<api::Health>) -> Element {
    let (class, text) = match health {
        Some(h) if h.database && h.schema => ("ok", "database connected, core schema present"),
        Some(h) if h.database => ("warn", "database connected, but the core schema is missing — apply the migrations"),
        Some(_) => ("err", "database unreachable"),
        None => ("warn", "status unknown"),
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
                    th { "Created" }
                }
            }
            tbody {
                for org in orgs {
                    tr { key: "{org.id}",
                        td { code { "{org.slug}" } }
                        td { "{org.name}" }
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
