use api::{
    connect_ai, connect_s3, delete_connection, health, list_connections, test_connection,
    validate_api_key, validate_base_url, validate_s3, Connection, Provider, TestResult,
};
use dioxus::prelude::*;

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::navigate_to;

const CONNECTIONS_CSS: Asset = asset!("/assets/styling/connections.css");

/// An org's connections and the forms to add more (docs/connections.md §3).
///
/// Nothing here ever holds a credential after submitting it: the forms send
/// keys to the server, which puts them in the vault and answers with the
/// connection's public description only.
#[component]
pub(crate) fn ConnectionsPanel(slug: ReadSignal<String>) -> Element {
    let mut list = use_server_future(move || list_connections(slug()))?;

    rsx! {
        document::Link { rel: "stylesheet", href: CONNECTIONS_CSS }
        Card {
            CardHeader {
                CardTitle { "Connections" }
                CardDescription {
                    "Credentials go straight to the vault — the app itself cannot read them back. "
                    "Test runs one read-only call through liaison, the only service allowed to use them."
                }
            }
            CardContent {
                match list() {
                    None => rsx! { p { "Loading…" } },
                    Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load connections: {e}" } },
                    Some(Ok(items)) if items.is_empty() => rsx! {
                        p { class: "orgs-empty", "Nothing connected yet." }
                    },
                    Some(Ok(items)) => rsx! {
                        div { class: "conn-list",
                            for connection in items {
                                ConnectionRow {
                                    key: "{connection.id}",
                                    slug: slug(),
                                    connection: connection.clone(),
                                    on_changed: move |_| list.restart(),
                                }
                            }
                        }
                    },
                }
            }
        }
        AddConnection { slug: slug(), on_added: move |_| list.restart() }
    }
}

#[component]
fn ConnectionRow(slug: String, connection: Connection, on_changed: EventHandler<()>) -> Element {
    let mut busy = use_signal(|| false);
    let mut result = use_signal(|| None::<TestResult>);
    let mut confirming = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let (id, s) = (connection.id.clone(), slug.clone());
    let test = move |_| {
        let (id, s) = (id.clone(), s.clone());
        async move {
            busy.set(true);
            error.set(None);
            match test_connection(s, id).await {
                Ok(r) => result.set(Some(r)),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
            on_changed.call(());
        }
    };
    let (id, s) = (connection.id.clone(), slug.clone());
    let remove = move |_| {
        let (id, s) = (id.clone(), s.clone());
        async move {
            if !confirming() {
                confirming.set(true);
                return;
            }
            busy.set(true);
            match delete_connection(s, id).await {
                Ok(()) => on_changed.call(()),
                Err(e) => {
                    error.set(Some(e.to_string()));
                    confirming.set(false);
                }
            }
            busy.set(false);
        }
    };

    let checked = connection
        .last_checked_at
        .clone()
        .map(|t| format!("tested {t}"))
        .unwrap_or_else(|| "never tested".to_string());

    rsx! {
        div { class: "conn-row",
            div { class: "conn-main",
                span { class: "conn-provider", "{connection.provider.name()}" }
                span { class: "conn-label", "{connection.label}" }
                span { class: "conn-status conn-status-{connection.status}", "{connection.status}" }
            }
            div { class: "conn-meta",
                "by {connection.owner_email} · {connection.created_at} · {checked} · "
                code { "{connection.base_url}" }
            }
            if let Some(last) = connection.last_error.clone() {
                if result().is_none() {
                    div { class: "conn-meta orgs-error", "{last}" }
                }
            }
            if let Some(r) = result() {
                div { class: if r.ok { "conn-result ok" } else { "conn-result err" }, "{r.message}" }
            }
            if let Some(e) = error() {
                div { class: "conn-result err", "{e}" }
            }
            div { class: "conn-actions",
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Outline,
                    disabled: busy(),
                    onclick: test,
                    "Test"
                }
                if connection.can_remove {
                    Button {
                        size: ButtonSize::Sm,
                        variant: if confirming() { ButtonVariant::Destructive } else { ButtonVariant::Ghost },
                        disabled: busy(),
                        onclick: remove,
                        if confirming() { "Confirm removal" } else { "Remove" }
                    }
                }
            }
        }
    }
}

/// The three ways to add a connection: OAuth (GitHub, Google Drive), an S3
/// access key, or an AI provider's API token.
#[component]
fn AddConnection(slug: String, on_added: EventHandler<()>) -> Element {
    let status = use_server_future(health)?;
    let h = status().and_then(|r| r.ok());
    let vault = h.as_ref().is_some_and(|h| h.vault);
    let github = vault && h.as_ref().is_some_and(|h| h.github);
    let google = vault && h.as_ref().is_some_and(|h| h.google);

    let (gh_slug, gd_slug) = (slug.clone(), slug.clone());

    rsx! {
        Card {
            CardHeader {
                CardTitle { "Add a connection" }
                if !vault {
                    CardDescription { class: "orgs-error",
                        "Connections are disabled until the vault is configured (SECRETS_URL, SECRETS_PASSWORD)."
                    }
                }
            }
            CardContent {
                div { class: "conn-section",
                    h4 { "Accounts" }
                    div { class: "conn-oauth",
                        Button {
                            disabled: !github,
                            onclick: move |_| navigate_to(&format!("/auth/connect/github?org={gh_slug}")),
                            "Connect GitHub"
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            disabled: !google,
                            onclick: move |_| navigate_to(&format!("/auth/connect/gdrive?org={gd_slug}")),
                            "Connect Google Drive"
                        }
                    }
                }
                S3Form { slug: slug.clone(), disabled: !vault, on_added }
                AiForm { slug: slug.clone(), disabled: !vault, on_added }
            }
        }
    }
}

#[component]
fn S3Form(slug: String, disabled: bool, on_added: EventHandler<()>) -> Element {
    let mut endpoint = use_signal(|| "https://s3.fr-par.scw.cloud".to_string());
    let mut region = use_signal(|| "fr-par".to_string());
    let mut bucket = use_signal(String::new);
    let mut key_id = use_signal(String::new);
    let mut secret = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move |evt: FormEvent| {
        let slug = slug.clone();
        async move {
            evt.prevent_default();
            if let Err(message) =
                validate_s3(&endpoint(), &region(), &bucket(), &key_id(), &secret())
            {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match connect_s3(slug, endpoint(), region(), bucket(), key_id(), secret()).await {
                Ok(_) => {
                    bucket.set(String::new());
                    key_id.set(String::new());
                    secret.set(String::new());
                    error.set(None);
                    on_added.call(());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        }
    };

    rsx! {
        form { class: "conn-section", onsubmit: submit,
            h4 { "S3-compatible bucket" }
            div { class: "conn-grid",
                div { class: "orgs-field",
                    Label { html_for: "s3-endpoint", "Endpoint" }
                    Input { id: "s3-endpoint", value: endpoint(), oninput: move |e: FormEvent| endpoint.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "s3-region", "Region" }
                    Input { id: "s3-region", value: region(), oninput: move |e: FormEvent| region.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "s3-bucket", "Bucket" }
                    Input { id: "s3-bucket", placeholder: "my-bucket", value: bucket(), oninput: move |e: FormEvent| bucket.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "s3-key-id", "Access key id" }
                    Input { id: "s3-key-id", autocomplete: "off", value: key_id(), oninput: move |e: FormEvent| key_id.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "s3-secret", "Secret access key" }
                    Input { id: "s3-secret", r#type: "password", autocomplete: "off", value: secret(), oninput: move |e: FormEvent| secret.set(e.value()) }
                }
            }
            Button { r#type: "submit", disabled: disabled || busy(), "Connect bucket" }
            if let Some(message) = error() {
                p { class: "orgs-error", "{message}" }
            }
        }
    }
}

#[component]
fn AiForm(slug: String, disabled: bool, on_added: EventHandler<()>) -> Element {
    let mut provider = use_signal(|| Provider::Mistral);
    let mut base_url = use_signal(String::new);
    let mut key = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move |evt: FormEvent| {
        let slug = slug.clone();
        async move {
            evt.prevent_default();
            let p = provider();
            let checked = validate_api_key(&key()).and_then(|_| {
                if p == Provider::OpenaiCompatible {
                    validate_base_url(&base_url(), true).map(|_| ())
                } else {
                    Ok(())
                }
            });
            if let Err(message) = checked {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match connect_ai(slug, p, key(), base_url()).await {
                Ok(_) => {
                    key.set(String::new());
                    error.set(None);
                    on_added.call(());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        }
    };

    rsx! {
        form { class: "conn-section", onsubmit: submit,
            h4 { "AI account" }
            div { class: "conn-choices", role: "radiogroup",
                for p in Provider::AI {
                    Button {
                        key: "{p.id()}",
                        r#type: "button",
                        size: ButtonSize::Sm,
                        role: "radio",
                        aria_checked: provider() == p,
                        variant: if provider() == p { ButtonVariant::Primary } else { ButtonVariant::Outline },
                        onclick: move |_| provider.set(p),
                        "{p.name()}"
                    }
                }
            }
            div { class: "conn-grid",
                if provider() == Provider::OpenaiCompatible {
                    div { class: "orgs-field",
                        Label { html_for: "ai-base-url", "Base URL" }
                        Input {
                            id: "ai-base-url",
                            placeholder: "https://api.scaleway.ai/v1",
                            value: base_url(),
                            oninput: move |e: FormEvent| base_url.set(e.value()),
                        }
                    }
                } else if let Some(fixed) = provider().fixed_base_url() {
                    p { class: "conn-meta", "Calls go to " code { "{fixed}" } }
                }
                div { class: "orgs-field",
                    Label { html_for: "ai-key", "API key" }
                    Input {
                        id: "ai-key",
                        r#type: "password",
                        autocomplete: "off",
                        value: key(),
                        oninput: move |e: FormEvent| key.set(e.value()),
                    }
                }
            }
            Button { r#type: "submit", disabled: disabled || busy(), "Connect {provider().name()}" }
            if let Some(message) = error() {
                p { class: "orgs-error", "{message}" }
            }
        }
    }
}
