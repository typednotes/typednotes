use api::{
    ai_environment, aws_s3_endpoint, connect_ai, connect_azure, connect_caldav, connect_jmap,
    connect_notion, connect_s3, delete_connection, health, list_connections, test_connection,
    validate_api_key, validate_azure, validate_base_url, validate_caldav, validate_jmap,
    validate_name, validate_s3, Connection, Health, Provider, TestResult,
    connector_operations, connector_scope_label, set_connection_permissions,
    ConnectorPermissions, ConnectorScope, PermissionPreset,
};
use dioxus::prelude::*;

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::components::select::{Select, SelectOption};
use crate::components::textarea::Textarea;
use crate::error_message;
use crate::navigate_to;
use crate::permission_check::PermissionCheck;

pub(crate) const CONNECTIONS_CSS: Asset = asset!("/assets/styling/connections.css");

/// An org's connections and the forms to add more (docs/connections.md §3).
///
/// Nothing here ever holds a credential after submitting it: the forms send
/// keys to the server, which puts them in the vault and answers with the
/// connection's public description only.
#[component]
pub(crate) fn ConnectionsPanel(slug: ReadSignal<String>) -> Element {
    let mut list = use_resource(move || list_connections(slug()));
    let settings = use_resource(move || api::get_org_settings(slug()));

    rsx! {
        document::Link { rel: "stylesheet", href: CONNECTIONS_CSS }
        Card {
            CardHeader {
                CardTitle { "Connections" }
                CardDescription {
                    "Credentials go straight to the vault — the app itself cannot read them back. "
                    "Test runs one read-only call through the credential broker, the only service allowed to use them."
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
                                    policy: settings().and_then(|s| s.ok()).map(|s| s.effect_policy),
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
fn ConnectionRow(slug: String, connection: Connection, policy: Option<api::EffectPolicy>, on_changed: EventHandler<()>) -> Element {
    let mut busy = use_signal(|| false);
    let mut result = use_signal(|| None::<TestResult>);
    let mut confirming = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut show_models = use_signal(|| false);
    let price_model = use_signal(String::new);
    let parent = connection.permissions.clone().unwrap_or_else(|| ConnectorPermissions::preset(connection.provider, PermissionPreset::ReadOnly));
    let operation = if connection.provider.is_ai() { "models.list" } else if connection.provider.is_code() { "repositories.list" } else { "" };
    let blocked = policy.as_ref().and_then(|policy| {
        if !policy.allows_connector(connection.provider) || !operation.is_empty() {
            policy.connector_blocker(connection.provider, &parent, operation, &[])
        } else { None }
    });

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
            if let Some(message) = blocked.as_ref() {
                p { class: "conn-result err", "{message} ",
                    Link { to: "/orgs/{slug}/settings/notebooks", "Review notebook permissions" }
                }
            }
            div { class: "conn-actions",
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Outline,
                    disabled: busy() || blocked.is_some(),
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
            ConnectionPermissions { key: "{connection.permissions:?}", slug: slug.clone(), connection: connection.clone(), on_changed }
            if connection.provider.is_ai() {
                Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Outline,
                    onclick: move |_| show_models.set(!show_models()),
                    if show_models() { "Hide models" } else { "Models and pricing" }
                }
                if show_models() {
                    crate::ai_models::ModelPicker { key: "{connection.id}", slug: slug.clone(), connection_id: connection.id.clone(), value: price_model }
                    crate::ai_cost::ModelCost { provider: connection.provider, model: price_model() }
                }
            }
            if connection.provider == Provider::TypeSafe {
                ClassifierForm { slug: slug.clone(), connection_id: connection.id.clone() }
            }
        }
    }
}

/// Shared policy editor. `None` is inheritance, never an unrestricted grant.
/// A cell's presets intersect its ceiling; execution still validates authority.
#[component]
pub(crate) fn PermissionEditor(
    id: String,
    provider: Provider,
    value: Option<ConnectorPermissions>,
    ceiling: Option<ConnectorPermissions>,
    on_change: EventHandler<Option<ConnectorPermissions>>,
    #[props(default = true)] allow_inherit: bool,
    #[props(default = false)] disabled: bool,
) -> Element {
    let displayed = value.clone().or_else(|| ceiling.clone()).unwrap_or_else(|| ConnectorPermissions::preset(provider, PermissionPreset::ReadOnly));
    let preset = |preset| {
        let requested = ConnectorPermissions::preset(provider, preset);
        ceiling.as_ref().map_or(requested.clone(), |limit| limit.intersect(&requested))
    };
    let read = preset(PermissionPreset::ReadOnly);
    let write = preset(PermissionPreset::ReadWrite);
    let common_root = displayed.scopes.first().map(|s| s.root.clone())
        .filter(|root| displayed.scopes.iter().all(|s| &s.root == root));
    let mixed_resources = !displayed.scopes.is_empty() && common_root.is_none();
    rsx! {
        div { class: "permission-editor",
            div { class: "conn-actions",
                if allow_inherit {
                    Button { r#type: "button", size: ButtonSize::Sm, disabled, aria_pressed: value.is_none().to_string(),
                        variant: if value.is_none() { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                        onclick: move |_| on_change.call(None), "Inherit permissions" }
                }
                Button { r#type: "button", size: ButtonSize::Sm, disabled, aria_pressed: (value.as_ref() == Some(&read)).to_string(),
                    variant: if value.as_ref() == Some(&read) { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                    onclick: { let read = read.clone(); move |_| on_change.call(Some(read.clone())) }, "Read only" }
                Button { r#type: "button", size: ButtonSize::Sm, disabled, aria_pressed: (value.as_ref() == Some(&write)).to_string(),
                    variant: if value.as_ref() == Some(&write) { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                    onclick: { let write = write.clone(); move |_| on_change.call(Some(write.clone())) }, "Read and write" }
            }
            p { class: "conn-meta",
                if value.is_none() { "Uses the connection ceiling and organization policy. Unset connections default to read only." }
                else { "Read and write excludes delete, share, invite and send. An empty operation list denies access." }
            }
            div { class: "orgs-field permission-boundary",
                Label { html_for: "{id}-boundary", "Resource boundary (optional)" }
                Input { id: "{id}-boundary", disabled: disabled || mixed_resources,
                    value: common_root.unwrap_or_default().join("/"),
                    placeholder: match provider {
                        Provider::S3 | Provider::Azure => "reports/2026",
                        Provider::Github | Provider::Gitlab => "owner/repository",
                        Provider::GoogleCalendar | Provider::MicrosoftCalendar | Provider::Caldav => "primary",
                        Provider::Gdrive | Provider::Dropbox => "folder ID or folder/path",
                        Provider::Gmail | Provider::Outlook | Provider::Jmap => "mailbox or label ID",
                        Provider::Slack | Provider::Whatsapp | Provider::Signal => "channel or recipient ID",
                        Provider::Notion => "page or database ID",
                        _ => "model ID",
                    },
                    oninput: { let displayed = displayed.clone(); move |event: FormEvent| {
                        let mut next = displayed.clone();
                        let root = if event.value().is_empty() { Vec::new() } else { event.value().split('/').map(str::to_string).collect() };
                        for scope in &mut next.scopes { scope.root = root.clone(); }
                        on_change.call(Some(next));
                    } }
                }
                p { class: "conn-meta",
                    if mixed_resources { "Several resource boundaries are set. Edit them separately in Advanced." }
                    else { "Applies to all selected operations. Leave empty to use the connection's base resource; it never grants access beyond that credential. Advanced can give each operation different resources." }
                }
            }
            if let Some(limit) = &ceiling {
                if let Err(message) = limit.narrow(&displayed) {
                    p { class: "orgs-error", role: "alert", "{message}. Choose a resource inside the inherited boundary." }
                }
            }
            details { class: "permission-advanced",
                summary { "Advanced operations and resources" }
                p { class: "conn-meta", "Requested policy grants; the broker checks operation support, scopes and credentials when a call runs." }
                div { class: "permission-options",
                    for operation in connector_operations(provider) {
                        PermissionCheck { id: "{id}-{operation.id}", label: operation.label,
                            disabled: disabled || (ceiling.as_ref().is_some_and(|c| !c.scopes.iter().any(|s| s.operation == operation.id)) && !displayed.scopes.iter().any(|s| s.operation == operation.id)),
                            checked: displayed.scopes.iter().any(|s| s.operation == operation.id),
                            on_change: {
                                let mut next = displayed.clone();
                                let ceiling = ceiling.clone();
                                move |_: bool| {
                                    if next.scopes.iter().any(|s| s.operation == operation.id) { next.scopes.retain(|s| s.operation != operation.id); }
                                    else if let Some(limit) = &ceiling {
                                        next.scopes.extend(limit.scopes.iter().filter(|s| s.operation == operation.id).cloned());
                                    } else { next.scopes.push(ConnectorScope { operation: operation.id.into(), root: Vec::new(), descendants: true }); }
                                    on_change.call(Some(next.clone()));
                                }
                            } }
                    }
                }
                p { class: "conn-meta", "{connector_scope_label(provider)}. Separate resource levels with /; an empty root means the credential's base resource. Scopes match whole components, not text prefixes." }
                for (index, grant) in displayed.scopes.iter().enumerate() {
                    div { class: "permission-grant", key: "{index}-{grant.operation}",
                        div { class: "orgs-field",
                            Label { html_for: "{id}-scope-{index}", "{grant.operation} resource" }
                            Input { id: "{id}-scope-{index}", disabled, value: grant.root.join("/"), placeholder: "reports/2026",
                                oninput: { let mut next = displayed.clone(); move |event: FormEvent| {
                                    next.scopes[index].root = if event.value().is_empty() { Vec::new() } else { event.value().split('/').map(str::to_string).collect() };
                                    on_change.call(Some(next.clone()));
                                } } }
                        }
                        PermissionCheck { id: "{id}-descendants-{index}", label: "Include descendants", disabled, checked: grant.descendants,
                            on_change: { let mut next = displayed.clone(); move |checked: bool| { next.scopes[index].descendants = checked; on_change.call(Some(next.clone())); } } }
                        Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Ghost, disabled,
                            aria_label: "Add another resource for {grant.operation}",
                            onclick: { let mut next = displayed.clone(); move |_| { next.scopes.push(next.scopes[index].clone()); on_change.call(Some(next.clone())); } }, "Add resource" }
                        Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Ghost, disabled,
                            aria_label: "Remove {grant.operation} resource {index + 1}",
                            onclick: { let mut next = displayed.clone(); move |_| { next.scopes.remove(index); on_change.call(Some(next.clone())); } }, "Remove grant" }
                    }
                }
                div { class: "conn-grid",
                    div { class: "orgs-field",
                        Label { html_for: "{id}-request", "Maximum request bytes" }
                        Input { id: "{id}-request", r#type: "number", min: "1", max: "67108864", disabled, value: "{displayed.max_request_bytes}",
                            oninput: { let mut next = displayed.clone(); move |event: FormEvent| { if let Ok(bytes) = event.value().parse() { next.max_request_bytes = bytes; on_change.call(Some(next.clone())); } } } }
                    }
                    div { class: "orgs-field",
                        Label { html_for: "{id}-response", "Maximum response bytes" }
                        Input { id: "{id}-response", r#type: "number", min: "1", max: "67108864", disabled, value: "{displayed.max_response_bytes}",
                            oninput: { let mut next = displayed.clone(); move |event: FormEvent| { if let Ok(bytes) = event.value().parse() { next.max_response_bytes = bytes; on_change.call(Some(next.clone())); } } } }
                    }
                }
            }
        }
    }
}

#[component]
fn ConnectionPermissions(slug: String, connection: Connection, on_changed: EventHandler<()>) -> Element {
    let mut permissions = use_signal(|| connection.permissions.clone().unwrap_or_else(|| ConnectorPermissions::preset(connection.provider, PermissionPreset::ReadOnly)));
    let mut busy = use_signal(|| false);
    let mut outcome = use_signal(|| None::<Result<String, String>>);
    rsx! {
        details { class: "connection-permissions",
            summary { "Connection permissions" }
            p { class: "conn-meta", "Base resource: " code { "{connection.base_url}" } ". Bucket, container and calendar-collection credentials keep this boundary automatically. OAuth scopes may further limit operations." }
            PermissionEditor { id: "connection-{connection.id}", provider: connection.provider, value: Some(permissions()), ceiling: None,
                allow_inherit: false, disabled: !connection.can_remove || busy(),
                on_change: move |value: Option<ConnectorPermissions>| { if let Some(value) = value { permissions.set(value); } } }
            if connection.can_remove {
                Button { size: ButtonSize::Sm, disabled: busy(), onclick: move |_| {
                    let (slug, id) = (slug.clone(), connection.id.clone());
                    async move {
                        busy.set(true);
                        match set_connection_permissions(slug, id, permissions()).await {
                            Ok(_) => { outcome.set(Some(Ok("Connection permissions saved.".into()))); on_changed.call(()); }
                            Err(error) => outcome.set(Some(Err(error_message(&error)))),
                        }
                        busy.set(false);
                    }
                }, "Save connection permissions" }
            } else { p { class: "conn-meta", "Only the creator or an organization admin can change this ceiling." } }
            match outcome() {
                Some(Ok(message)) => rsx! { p { class: "conn-result ok", role: "status", "{message}" } },
                Some(Err(message)) => rsx! { p { class: "orgs-error", role: "alert", "{message}" } },
                None => rsx! {},
            }
        }
    }
}

#[component]
fn ClassifierForm(slug: String, connection_id: String) -> Element {
    let mut state = use_signal(String::new);
    let mut question = use_signal(|| "Does this convey urgency?".to_string());
    let mut answer = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let model = use_signal(|| "jev-latest".to_string());
    let (model_slug, model_connection) = (slug.clone(), connection_id.clone());
    let evaluate = move |_| {
        let (slug, id) = (slug.clone(), connection_id.clone());
        async move {
            busy.set(true);
            error.set(None);
            let request = serde_json::json!({"model":model(),"state":state(),
                "questions":{"answer":{"type":"noul","instructions":question()}}});
            match api::classify_with_ai(slug, id, request).await {
                Ok(value) => answer.set(Some(
                    serde_json::to_string_pretty(&value).unwrap_or_default(),
                )),
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };
    rsx! {
        div { class: "conn-subform",
            h4 { "Typed classification" }
            crate::ai_models::ModelPicker { key: "{model_connection}", slug: model_slug.clone(), connection_id: model_connection.clone(), value: model, disabled: busy() }
            div { class: "orgs-field",
                Label { html_for: "classifier-state", "Text / state to evaluate" }
                Textarea { id: "classifier-state", value: state(), oninput: move |e: FormEvent| state.set(e.value()) }
            }
            div { class: "orgs-field",
                Label { html_for: "classifier-question", "Yes/no question" }
                Input { id: "classifier-question", value: question(), oninput: move |e: FormEvent| question.set(e.value()) }
            }
            p { class: "conn-meta", "Jev's published rate is $0.042 per million input tokens; output tokens are free. Evaluations also use Typednotes model-call credits." }
            Button { variant: ButtonVariant::Outline, disabled: busy() || state().trim().is_empty() || question().trim().is_empty(), onclick: evaluate, "Evaluate question" }
            if let Some(value) = answer() { pre { "{value}" } }
            if let Some(message) = error() { p { class: "orgs-error", "{message}" } }
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
            Provider::Gdrive | Provider::GoogleCalendar | Provider::Gmail => h.google,
            Provider::MicrosoftCalendar | Provider::Outlook => h.microsoft,
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

/// The ways to add a connection, grouped by what it is for.
#[component]
fn AddConnection(slug: String, on_added: EventHandler<()>) -> Element {
    let status = use_resource(health);
    let h = status().and_then(|r| r.ok());
    let vault = h.as_ref().is_some_and(|h| h.vault);
    let mut storage = use_signal(|| Provider::S3);
    let mut calendar = use_signal(|| Provider::GoogleCalendar);
    let mut mail = use_signal(|| Provider::Gmail);

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
                div { class: "conn-section",
                    h4 { "Calendars" }
                    ProviderSelect {
                        options: Provider::CALENDARS.to_vec(),
                        value: calendar(),
                        on_change: move |p| calendar.set(p),
                        label: "Calendar provider",
                    }
                    if calendar() == Provider::Caldav {
                        CaldavForm { slug: slug.clone(), disabled: !vault, on_added }
                    } else {
                        ProductivityOAuth { slug: slug.clone(), provider: calendar(), health: h.clone() }
                    }
                }
                div { class: "conn-section",
                    h4 { "Webmail" }
                    ProviderSelect {
                        options: Provider::MAIL.to_vec(),
                        value: mail(),
                        on_change: move |p| mail.set(p),
                        label: "Webmail provider",
                    }
                    if mail() == Provider::Jmap {
                        JmapForm { slug: slug.clone(), disabled: !vault, on_added }
                    } else {
                        ProductivityOAuth { slug: slug.clone(), provider: mail(), health: h.clone() }
                    }
                }
                div { class: "conn-section",
                    h4 { "Workspaces" }
                    NotionForm { slug: slug.clone(), disabled: !vault, on_added }
                }
                AiForm { slug: slug.clone(), disabled: !vault, on_added }
            }
        }
    }
}

#[component]
fn ProductivityOAuth(slug: String, provider: Provider, health: Option<Health>) -> Element {
    let url = connect_url(provider, &slug, None);
    let ready = oauth_ready(health.as_ref(), provider);
    let note = match provider {
        Provider::GoogleCalendar | Provider::MicrosoftCalendar => {
            "Read calendars and events. Creating or changing events is not requested."
        }
        _ => "Read messages and folders. Sending, changing or deleting mail is not requested.",
    };
    let client = if matches!(provider, Provider::MicrosoftCalendar | Provider::Outlook) {
        "MICROSOFT"
    } else {
        "GOOGLE"
    };
    rsx! {
        p { class: "conn-meta", "{note} You will grant access on {provider.name()} and return here." }
        if matches!(provider, Provider::MicrosoftCalendar | Provider::Outlook) {
            p { class: "conn-meta", "Supports Outlook.com and Microsoft 365 work or school accounts." }
        }
        if !ready && health.as_ref().is_some_and(|h| h.vault) {
            p { class: "conn-meta orgs-error", "Set {client}_CLIENT_ID and {client}_CLIENT_SECRET on the deployment to enable this connection." }
        }
        Button {
            disabled: !ready,
            onclick: move |_| navigate_to(&url),
            "Connect {provider.name()}"
        }
    }
}

#[component]
fn CaldavForm(slug: String, disabled: bool, on_added: EventHandler<()>) -> Element {
    let mut endpoint = use_signal(String::new);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let submit = move |evt: FormEvent| {
        let slug = slug.clone();
        async move {
            evt.prevent_default();
            if let Err(message) = validate_caldav(&endpoint(), &username(), &password()) {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match connect_caldav(slug, endpoint(), username(), password()).await {
                Ok(_) => {
                    password.set(String::new());
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
            p { class: "conn-meta", "Connect Nextcloud, Fastmail or another CalDAV server using its calendar collection or calendar-home URL and an app password." }
            div { class: "conn-grid",
                div { class: "orgs-field",
                    Label { html_for: "caldav-url", "Calendar URL" }
                    Input { id: "caldav-url", placeholder: "https://cloud.example.com/remote.php/dav/calendars/me/personal/", value: endpoint(), oninput: move |e: FormEvent| endpoint.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "caldav-user", "Username" }
                    Input { id: "caldav-user", value: username(), oninput: move |e: FormEvent| username.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "caldav-password", "App password" }
                    Input { id: "caldav-password", r#type: "password", autocomplete: "off", value: password(), oninput: move |e: FormEvent| password.set(e.value()) }
                }
            }
            Button { r#type: "submit", disabled: disabled || busy(), "Connect CalDAV" }
            if let Some(message) = error() { p { class: "orgs-error", "{message}" } }
        }
    }
}

#[component]
fn JmapForm(slug: String, disabled: bool, on_added: EventHandler<()>) -> Element {
    let mut endpoint = use_signal(|| "https://api.fastmail.com/jmap/session".to_string());
    let mut token = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let submit = move |evt: FormEvent| {
        let slug = slug.clone();
        async move {
            evt.prevent_default();
            if let Err(message) = validate_jmap(&endpoint(), &token()) {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match connect_jmap(slug, endpoint(), token()).await {
                Ok(_) => {
                    token.set(String::new());
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
            p { class: "conn-meta", "Fastmail: Settings → Privacy & Security → Manage API tokens. Give the token mail access. Other JMAP servers must support bearer tokens and serve the session and API on the same origin." }
            div { class: "conn-grid",
                div { class: "orgs-field",
                    Label { html_for: "jmap-url", "JMAP session URL" }
                    Input { id: "jmap-url", value: endpoint(), oninput: move |e: FormEvent| endpoint.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "jmap-token", "API token" }
                    Input { id: "jmap-token", r#type: "password", autocomplete: "off", value: token(), oninput: move |e: FormEvent| token.set(e.value()) }
                }
            }
            Button { r#type: "submit", disabled: disabled || busy(), "Connect JMAP mail" }
            if let Some(message) = error() { p { class: "orgs-error", "{message}" } }
        }
    }
}

#[component]
fn NotionForm(slug: String, disabled: bool, on_added: EventHandler<()>) -> Element {
    let mut label = use_signal(|| "Notion workspace".to_string());
    let mut token = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let submit = move |evt: FormEvent| {
        let slug = slug.clone();
        async move {
            evt.prevent_default();
            if let Err(message) =
                validate_name(&label()).and_then(|_| validate_api_key(&token()).map(|_| ()))
            {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match connect_notion(slug, label(), token()).await {
                Ok(_) => {
                    token.set(String::new());
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
            p { class: "conn-meta",
                "Create an internal integration in "
                a { href: "https://www.notion.so/profile/integrations", target: "_blank", rel: "noopener noreferrer", "Notion's integrations settings" }
                ", choose its capabilities, then share the pages and databases it should access with that integration. Personal access tokens are also supported."
            }
            div { class: "conn-grid",
                div { class: "orgs-field",
                    Label { html_for: "notion-label", "Connection name" }
                    Input { id: "notion-label", value: label(), oninput: move |e: FormEvent| label.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "notion-token", "Integration or personal access token" }
                    Input { id: "notion-token", r#type: "password", autocomplete: "off", value: token(), oninput: move |e: FormEvent| token.set(e.value()) }
                }
            }
            Button { r#type: "submit", disabled: disabled || busy(), "Connect Notion" }
            if let Some(message) = error() { p { class: "orgs-error", "{message}" } }
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
    let mut provider = use_signal(|| Provider::Anthropic);
    let mut base_url = use_signal(String::new);
    let mut key = use_signal(String::new);
    let env_slug = slug.clone();
    let environment = use_resource(move || ai_environment(env_slug.clone()));
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let key_name = provider().ai_info().map(|p| p.env).unwrap_or("API_KEY");

    let submit = move |evt: FormEvent| {
        let slug = slug.clone();
        async move {
            evt.prevent_default();
            let p = provider();
            let checked = if key().trim().is_empty()
                && environment().is_some_and(|v| v.is_ok_and(|ps| ps.contains(&p)))
            {
                Ok(String::new())
            } else {
                validate_api_key(&key())
            };
            let checked = checked.and_then(|_| {
                if !base_url().trim().is_empty() || p.fixed_base_url().is_none() {
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
                on_change: move |p| {
                    provider.set(p); key.set(String::new()); base_url.set(String::new());
                },
                label: "AI provider",
            }
            div { class: "conn-grid",
                    div { class: "orgs-field",
                        Label { html_for: "ai-base-url", "Base URL (optional override)" }
                        Input {
                            id: "ai-base-url",
                            placeholder: provider().fixed_base_url().unwrap_or("https://api.example.com/v1"),
                            value: base_url(),
                            oninput: move |e: FormEvent| base_url.set(e.value()),
                        }
                    }
                div { class: "orgs-field",
                    Label { html_for: "ai-key", "API key / {key_name}" }
                    Input {
                        id: "ai-key",
                        r#type: "password",
                        autocomplete: "off",
                        placeholder: provider().ai_info().map(|p| p.env).unwrap_or("API key"),
                        value: key(),
                        oninput: move |e: FormEvent| key.set(e.value()),
                    }
                }
            }
            if environment().is_some_and(|v| v.is_ok_and(|ps| ps.contains(&provider()))) {
                p { class: "conn-meta", "Leave the key empty to import the org-scoped deployment environment key." }
            }
            if provider() == Provider::Scaleway {
                p { class: "conn-meta", "Uses SCW_SECRET_KEY. For a non-default project, insert your project UUID between api.scaleway.ai/ and /v1. Dedicated Generative APIs and private NIM deployments may use their own endpoint." }
            }
            if provider() == Provider::TypeSafe {
                p { class: "conn-meta", "Typed classifier models use /systemone. This connection evaluates questions; it is not a code-writing model." }
            }
            p { class: "conn-meta", "After connecting, open Models and pricing to select from this provider's live catalog. Notebooks use the same model menu." }
            Button { r#type: "submit", disabled: disabled || busy(), "Connect {provider().name()}" }
            if let Some(message) = error() {
                p { class: "orgs-error", "{message}" }
            }
        }
    }
}
