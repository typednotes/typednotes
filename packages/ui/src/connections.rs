use api::{
    aws_s3_endpoint, connect_ai, connect_azure, connect_s3, delete_connection, health,
    list_connections, test_connection, validate_api_key, validate_azure, validate_base_url,
    validate_s3, Connection, Health, Provider, TestResult,
};
use dioxus::prelude::*;

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::components::select::{Select, SelectOption};
use crate::error_message;
use crate::navigate_to;

pub(crate) const CONNECTIONS_CSS: Asset = asset!("/assets/styling/connections.css");

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
                    Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load connections: {error_message(&e)}" } },
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
                Err(e) => error.set(Some(error_message(&e))),
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
                    error.set(Some(error_message(&e)));
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

/// Whether an OAuth provider can be connected here: the vault is configured
/// and so is the provider's OAuth client.
pub(crate) fn oauth_ready(h: Option<&Health>, provider: Provider) -> bool {
    let Some(h) = h else { return false };
    h.vault
        && match provider {
            Provider::Github => h.github,
            Provider::Gitlab => h.gitlab,
            Provider::Gdrive => h.google,
            Provider::Dropbox => h.dropbox,
            Provider::Slack => h.slack,
            _ => false,
        }
}

/// The OAuth connect route for `provider`, optionally coming back to a
/// project page. Only built from ids and validated slugs.
pub(crate) fn connect_url(provider: Provider, org: &str, project: Option<&str>) -> String {
    match project {
        Some(p) => format!("/auth/connect/{}?org={org}&project={p}", provider.id()),
        None => format!("/auth/connect/{}?org={org}", provider.id()),
    }
}

/// A menu of providers, in the order given.
#[component]
fn ProviderSelect(
    options: Vec<Provider>,
    value: Provider,
    on_change: EventHandler<Provider>,
    label: String,
) -> Element {
    rsx! {
        div { class: "conn-select",
            Select::<Provider> {
                default_value: value,
                aria_label: "{label}",
                on_value_change: move |v: Option<Provider>| {
                    if let Some(p) = v {
                        on_change.call(p);
                    }
                },
                for (i, p) in options.into_iter().enumerate() {
                    SelectOption::<Provider> {
                        key: "{p.id()}",
                        index: i,
                        value: p,
                        text_value: "{p.name()}",
                        "{p.name()}"
                    }
                }
            }
        }
    }
}

/// The ways to add a connection, by what it is for: code (GitHub, GitLab),
/// storage (S3, Azure, Dropbox, Google Drive) and AI.
#[component]
fn AddConnection(slug: String, on_added: EventHandler<()>) -> Element {
    let status = use_server_future(health)?;
    let h = status().and_then(|r| r.ok());
    let vault = h.as_ref().is_some_and(|h| h.vault);
    let mut storage = use_signal(|| Provider::S3);

    let oauth_button = |provider: Provider, primary: bool| {
        let url = connect_url(provider, &slug, None);
        rsx! {
            Button {
                variant: if primary { ButtonVariant::Primary } else { ButtonVariant::Outline },
                disabled: !oauth_ready(h.as_ref(), provider),
                onclick: move |_| navigate_to(&url),
                "Connect {provider.name()}"
            }
        }
    };

    rsx! {
        Card {
            CardHeader {
                CardTitle { "Add a connection" }
                if !vault {
                    CardDescription { class: "orgs-error",
                        "Connections are disabled: the vault is not configured (SECRETS_URL, SECRETS_PASSWORD) or refuses the app's login."
                    }
                } else {
                    CardDescription {
                        "Slack, WhatsApp and Signal are connected from a project's Interfaces."
                    }
                }
            }
            CardContent {
                div { class: "conn-section",
                    h4 { "Code" }
                    div { class: "conn-oauth",
                        {oauth_button(Provider::Github, true)}
                        {oauth_button(Provider::Gitlab, false)}
                    }
                }
                div { class: "conn-section",
                    h4 { "Storage" }
                    ProviderSelect {
                        options: Provider::STORAGE.to_vec(),
                        value: storage(),
                        on_change: move |p| storage.set(p),
                        label: "Storage provider",
                    }
                    match storage() {
                        Provider::S3 => rsx! { S3Form { slug: slug.clone(), disabled: !vault, on_added } },
                        Provider::Azure => rsx! { AzureForm { slug: slug.clone(), disabled: !vault, on_added } },
                        p => rsx! {
                            p { class: "conn-meta",
                                "You will be sent to {p.name()} to grant access, then back here."
                            }
                            {oauth_button(p, true)}
                        },
                    }
                }
                AiForm { slug: slug.clone(), disabled: !vault, on_added }
            }
        }
    }
}

/// Where an S3 bucket lives; each preset derives the endpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
enum S3Preset {
    Aws,
    Scaleway,
    CloudflareR2,
    Other,
}

impl S3Preset {
    const ALL: [S3Preset; 4] = [
        S3Preset::Aws,
        S3Preset::CloudflareR2,
        S3Preset::Scaleway,
        S3Preset::Other,
    ];

    fn name(self) -> &'static str {
        match self {
            S3Preset::Aws => "Amazon S3",
            S3Preset::Scaleway => "Scaleway Object Storage",
            S3Preset::CloudflareR2 => "Cloudflare R2",
            S3Preset::Other => "Other S3-compatible",
        }
    }

    fn default_region(self) -> &'static str {
        match self {
            S3Preset::Aws => "us-east-1",
            S3Preset::Scaleway => "fr-par",
            S3Preset::CloudflareR2 => "auto",
            S3Preset::Other => "",
        }
    }
}

#[component]
fn S3Form(slug: String, disabled: bool, on_added: EventHandler<()>) -> Element {
    let mut preset = use_signal(|| S3Preset::Aws);
    let mut region = use_signal(|| S3Preset::Aws.default_region().to_string());
    let mut custom_endpoint = use_signal(String::new);
    let mut r2_account = use_signal(String::new);
    let mut bucket = use_signal(String::new);
    let mut key_id = use_signal(String::new);
    let mut secret = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let endpoint = use_memo(move || match preset() {
        S3Preset::Aws => aws_s3_endpoint(&region()),
        S3Preset::Scaleway => format!("https://s3.{}.scw.cloud", region().trim()),
        S3Preset::CloudflareR2 => {
            format!("https://{}.r2.cloudflarestorage.com", r2_account().trim())
        }
        S3Preset::Other => custom_endpoint(),
    });

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
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };

    rsx! {
        form { class: "conn-subform", onsubmit: submit,
            div { class: "conn-choices", role: "radiogroup", aria_label: "Where the bucket lives",
                for p in S3Preset::ALL {
                    Button {
                        key: "{p.name()}",
                        r#type: "button",
                        size: ButtonSize::Sm,
                        role: "radio",
                        aria_checked: preset() == p,
                        variant: if preset() == p { ButtonVariant::Primary } else { ButtonVariant::Outline },
                        onclick: move |_| {
                            preset.set(p);
                            if p != S3Preset::Other {
                                region.set(p.default_region().to_string());
                            }
                        },
                        "{p.name()}"
                    }
                }
            }
            div { class: "conn-grid",
                match preset() {
                    S3Preset::Other => rsx! {
                        div { class: "orgs-field",
                            Label { html_for: "s3-endpoint", "Endpoint" }
                            Input { id: "s3-endpoint", placeholder: "https://s3.example.com", value: custom_endpoint(), oninput: move |e: FormEvent| custom_endpoint.set(e.value()) }
                        }
                    },
                    S3Preset::CloudflareR2 => rsx! {
                        div { class: "orgs-field",
                            Label { html_for: "s3-r2-account", "Account id" }
                            Input { id: "s3-r2-account", placeholder: "Cloudflare account id", value: r2_account(), oninput: move |e: FormEvent| r2_account.set(e.value()) }
                        }
                    },
                    _ => rsx! {},
                }
                if preset() != S3Preset::CloudflareR2 {
                    div { class: "orgs-field",
                        Label { html_for: "s3-region", "Region" }
                        Input { id: "s3-region", placeholder: "us-east-1", value: region(), oninput: move |e: FormEvent| region.set(e.value()) }
                    }
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
            if preset() != S3Preset::Other {
                p { class: "conn-meta", "Calls go to " code { "{endpoint}/{bucket}" } }
            }
            Button { r#type: "submit", disabled: disabled || busy(), "Connect bucket" }
            if let Some(message) = error() {
                p { class: "orgs-error", "{message}" }
            }
        }
    }
}

#[component]
fn AzureForm(slug: String, disabled: bool, on_added: EventHandler<()>) -> Element {
    let mut account = use_signal(String::new);
    let mut container = use_signal(String::new);
    let mut sas = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move |evt: FormEvent| {
        let slug = slug.clone();
        async move {
            evt.prevent_default();
            if let Err(message) = validate_azure(&account(), &container(), &sas()) {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match connect_azure(slug, account(), container(), sas()).await {
                Ok(_) => {
                    container.set(String::new());
                    sas.set(String::new());
                    error.set(None);
                    on_added.call(());
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };

    rsx! {
        form { class: "conn-subform", onsubmit: submit,
            div { class: "conn-grid",
                div { class: "orgs-field",
                    Label { html_for: "az-account", "Storage account" }
                    Input { id: "az-account", placeholder: "mystorageaccount", value: account(), oninput: move |e: FormEvent| account.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "az-container", "Container" }
                    Input { id: "az-container", placeholder: "notes", value: container(), oninput: move |e: FormEvent| container.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "az-sas", "SAS token or SAS URL" }
                    Input { id: "az-sas", r#type: "password", autocomplete: "off", placeholder: "sv=…&sig=…", value: sas(), oninput: move |e: FormEvent| sas.set(e.value()) }
                }
            }
            p { class: "conn-meta",
                "Generate a container SAS (read, list, write as needed) in the Azure portal: "
                "Storage account → Containers → … → Generate SAS."
            }
            Button { r#type: "submit", disabled: disabled || busy(), "Connect container" }
            if let Some(message) = error() {
                p { class: "orgs-error", "{message}" }
            }
        }
    }
}

#[component]
fn AiForm(slug: String, disabled: bool, on_added: EventHandler<()>) -> Element {
    let mut provider = use_signal(|| Provider::AI[0]);
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
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };

    rsx! {
        form { class: "conn-section", onsubmit: submit,
            h4 { "AI" }
            ProviderSelect {
                options: Provider::AI.to_vec(),
                value: provider(),
                on_change: move |p| provider.set(p),
                label: "AI provider",
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
