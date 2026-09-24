use api::{
    add_slack_channel, connect_signal, connect_whatsapp, health, list_channels, list_connections,
    list_messages, list_slack_channels, remove_channel, send_message, validate_message,
    validate_phone, validate_signal, validate_whatsapp, Channel, Health, Message, Provider,
};
use dioxus::prelude::*;

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::components::select::{Select, SelectOption};
use crate::components::textarea::Textarea;
use crate::connections::{connect_url, oauth_ready};
use crate::error_message;
use crate::navigate_to;

/// A project's messaging interfaces and its inbox, which share the list of
/// channels: adding an interface makes it available to reply through.
#[component]
pub(crate) fn ChannelsSection(slug: ReadSignal<String>, project: ReadSignal<String>) -> Element {
    let mut channels = use_server_future(move || list_channels(slug(), project()))?;
    let mut inbox = use_server_future(move || list_messages(slug(), project()))?;
    let status = use_server_future(health)?;
    let h = status().and_then(|r| r.ok());
    let list: Vec<Channel> = match channels() {
        Some(Ok(list)) => list,
        _ => Vec::new(),
    };
    let load_error = match channels() {
        Some(Err(e)) => Some(error_message(&e)),
        _ => None,
    };
    // Where a reply goes, set by "Reply" on an inbound message.
    let reply_to = use_signal(|| None::<(String, String)>);

    rsx! {
        Card {
            CardHeader {
                CardTitle { "Interfaces" }
                CardDescription {
                    "Where people reach this project. Their messages land in the inbox below."
                }
            }
            CardContent {
                if let Some(e) = load_error {
                    p { class: "orgs-error", "Could not load interfaces: {e}" }
                }
                if list.is_empty() {
                    p { class: "orgs-empty", "No interface yet." }
                }
                div { class: "conn-list",
                    for channel in list.clone() {
                        ChannelRow {
                            key: "{channel.id}",
                            slug: slug(),
                            project: project(),
                            channel,
                            health: h.clone(),
                            on_removed: move |_| {
                                channels.restart();
                                inbox.restart();
                            },
                        }
                    }
                }
                AddInterface {
                    slug: slug(),
                    project: project(),
                    health: h.clone(),
                    on_added: move |_| channels.restart(),
                }
            }
        }
        Card {
            CardHeader {
                CardTitle { "Inbox" }
                CardDescription { "The latest messages through this project's interfaces, both ways." }
            }
            CardContent {
                div { class: "conn-actions",
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Outline,
                        onclick: move |_| inbox.restart(),
                        "Refresh"
                    }
                }
                match inbox() {
                    None => rsx! { p { "Loading…" } },
                    Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load the inbox: {error_message(&e)}" } },
                    Some(Ok(messages)) if messages.is_empty() => rsx! {
                        p { class: "orgs-empty", "No message yet." }
                    },
                    Some(Ok(messages)) => rsx! {
                        div { class: "inbox",
                            for message in messages {
                                MessageRow { key: "{message.id}", message, reply_to }
                            }
                        }
                    },
                }
                if !list.is_empty() {
                    Composer {
                        slug: slug(),
                        project: project(),
                        channels: list.clone(),
                        reply_to,
                        on_sent: move |_| inbox.restart(),
                    }
                }
            }
        }
    }
}

/// How messages reach a channel: the webhook it needs, or the pull.
fn delivery_note(provider: Provider, h: Option<&Health>) -> (String, bool) {
    let configured = |flag: bool, text: &str, missing: &str| {
        if flag {
            (text.to_string(), true)
        } else {
            (missing.to_string(), false)
        }
    };
    match provider {
        Provider::Slack => configured(
            h.is_some_and(|h| h.slack_events),
            "inbound: Slack Events API → /hooks/slack",
            "inbound disabled: set SLACK_SIGNING_SECRET and point the Slack app's Event Subscriptions to /hooks/slack",
        ),
        Provider::Whatsapp => configured(
            h.is_some_and(|h| h.whatsapp_webhook),
            "inbound: Meta webhook → /hooks/whatsapp",
            "inbound disabled: set WHATSAPP_APP_SECRET and WHATSAPP_VERIFY_TOKEN, and register /hooks/whatsapp in the Meta app",
        ),
        _ => (
            "inbound: fetched from the bridge when the inbox loads".to_string(),
            true,
        ),
    }
}

#[component]
fn ChannelRow(
    slug: String,
    project: String,
    channel: Channel,
    health: Option<Health>,
    on_removed: EventHandler<()>,
) -> Element {
    let mut confirming = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let (note, ok) = delivery_note(channel.provider, health.as_ref());
    let id = channel.id.clone();
    rsx! {
        div { class: "conn-row",
            div { class: "conn-main",
                span { class: "conn-provider", "{channel.provider.name()}" }
                span { class: "conn-label", "{channel.label}" }
            }
            div { class: if ok { "conn-meta" } else { "conn-meta orgs-error" }, "{note} · added {channel.created_at}" }
            div { class: "conn-actions",
                Button {
                    size: ButtonSize::Sm,
                    variant: if confirming() { ButtonVariant::Destructive } else { ButtonVariant::Ghost },
                    onclick: move |_| {
                        let (slug, project, id) = (slug.clone(), project.clone(), id.clone());
                        async move {
                            if !confirming() {
                                confirming.set(true);
                                return;
                            }
                            match remove_channel(slug, project, id).await {
                                Ok(()) => on_removed.call(()),
                                Err(e) => {
                                    error.set(Some(error_message(&e)));
                                    confirming.set(false);
                                }
                            }
                        }
                    },
                    if confirming() { "Confirm removal" } else { "Remove" }
                }
            }
            if let Some(e) = error() {
                div { class: "conn-result err", "{e}" }
            }
        }
    }
}

#[component]
fn AddInterface(
    slug: String,
    project: String,
    health: Option<Health>,
    on_added: EventHandler<()>,
) -> Element {
    let mut kind = use_signal(|| Provider::CHANNELS[0]);
    rsx! {
        div { class: "conn-section",
            h4 { "Add an interface" }
            div { class: "conn-select",
                Select::<Provider> {
                    default_value: kind(),
                    aria_label: "Messaging provider",
                    on_value_change: move |v: Option<Provider>| {
                        if let Some(p) = v {
                            kind.set(p);
                        }
                    },
                    for (i, p) in Provider::CHANNELS.into_iter().enumerate() {
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
            match kind() {
                Provider::Slack => rsx! { SlackForm { slug: slug.clone(), project: project.clone(), health: health.clone(), on_added } },
                Provider::Whatsapp => rsx! { WhatsappForm { slug: slug.clone(), project: project.clone(), on_added } },
                _ => rsx! { SignalForm { slug: slug.clone(), project: project.clone(), on_added } },
            }
        }
    }
}

/// Pick a workspace the Slack app is installed in (or install it), then a
/// channel its bot can post to.
#[component]
fn SlackForm(
    slug: String,
    project: String,
    health: Option<Health>,
    on_added: EventHandler<()>,
) -> Element {
    let org = slug.clone();
    let connections = use_server_future(move || list_connections(org.clone()))?;
    let workspaces: Vec<(String, String)> = match connections() {
        Some(Ok(list)) => list
            .into_iter()
            .filter(|c| c.provider == Provider::Slack)
            .map(|c| (c.id, c.label))
            .collect(),
        _ => Vec::new(),
    };
    let mut workspace = use_signal(|| None::<String>);
    let mut slack_channel = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let current = workspace().or_else(|| workspaces.first().map(|(id, _)| id.clone()));
    let install_url = connect_url(Provider::Slack, &slug, Some(&project));
    let ready = oauth_ready(health.as_ref(), Provider::Slack);

    let org = slug.clone();
    let chosen = current.clone();
    let available = use_resource(use_reactive!(|(chosen,)| {
        let org = org.clone();
        async move {
            match chosen {
                Some(id) => list_slack_channels(org, id)
                    .await
                    .map_err(|e| error_message(&e)),
                None => Ok(Vec::new()),
            }
        }
    }));

    let add = {
        let current = current.clone();
        move |_| {
            let (slug, project, current) = (slug.clone(), project.clone(), current.clone());
            async move {
                let (Some(connection), Some(channel)) = (current, slack_channel()) else {
                    error.set(Some("choose a workspace and a channel".to_string()));
                    return;
                };
                busy.set(true);
                match add_slack_channel(slug, project, connection, channel).await {
                    Ok(_) => {
                        error.set(None);
                        slack_channel.set(None);
                        on_added.call(());
                    }
                    Err(e) => error.set(Some(error_message(&e))),
                }
                busy.set(false);
            }
        }
    };

    rsx! {
        div { class: "conn-subform",
            if workspaces.is_empty() {
                p { class: "conn-meta", "Install the Typednotes Slack app in a workspace; you will come back here." }
            } else {
                div { class: "conn-picker",
                    div { class: "conn-select",
                        Select::<String> {
                            default_value: current.clone().unwrap_or_default(),
                            aria_label: "Slack workspace",
                            on_value_change: move |v: Option<String>| {
                                slack_channel.set(None);
                                workspace.set(v);
                            },
                            for (i, (id, label)) in workspaces.iter().enumerate() {
                                SelectOption::<String> { key: "{id}", index: i, value: id.clone(), text_value: label.clone(), "{label}" }
                            }
                        }
                    }
                    match available() {
                        None => rsx! { p { class: "conn-meta", "Loading channels…" } },
                        Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not list channels: {e}" } },
                        Some(Ok(list)) => rsx! {
                            div { class: "conn-select", key: "{current:?}",
                                Select::<String> {
                                    aria_label: "Slack channel",
                                    on_value_change: move |v: Option<String>| slack_channel.set(v),
                                    for (i, c) in list.iter().enumerate() {
                                        SelectOption::<String> {
                                            key: "{c.id}",
                                            index: i,
                                            value: c.id.clone(),
                                            text_value: format!("#{}", c.name),
                                            if c.private { "🔒 " } else { "#" }
                                            "{c.name}"
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
                Button { disabled: busy() || slack_channel().is_none(), onclick: add, "Add channel" }
            }
            Button {
                variant: ButtonVariant::Outline,
                disabled: !ready,
                onclick: move |_| navigate_to(&install_url),
                if workspaces.is_empty() { "Add to Slack" } else { "Add to another workspace" }
            }
            if let Some(e) = error() {
                p { class: "orgs-error", "{e}" }
            }
        }
    }
}

#[component]
fn WhatsappForm(slug: String, project: String, on_added: EventHandler<()>) -> Element {
    let mut phone_id = use_signal(String::new);
    let mut token = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move |evt: FormEvent| {
        let (slug, project) = (slug.clone(), project.clone());
        async move {
            evt.prevent_default();
            if let Err(message) = validate_whatsapp(&phone_id(), &token()) {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match connect_whatsapp(slug, project, phone_id(), token()).await {
                Ok(_) => {
                    phone_id.set(String::new());
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
            div { class: "conn-grid",
                div { class: "orgs-field",
                    Label { html_for: "wa-phone-id", "Phone number id" }
                    Input { id: "wa-phone-id", placeholder: "106540352242922", value: phone_id(), oninput: move |e: FormEvent| phone_id.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "wa-token", "Access token" }
                    Input { id: "wa-token", r#type: "password", autocomplete: "off", value: token(), oninput: move |e: FormEvent| token.set(e.value()) }
                }
            }
            p { class: "conn-meta",
                "From the WhatsApp Cloud API setup of your Meta app: the phone number id (not the number) "
                "and a system user's permanent access token with whatsapp_business_messaging."
            }
            Button { r#type: "submit", disabled: busy(), "Connect WhatsApp" }
            if let Some(message) = error() {
                p { class: "orgs-error", "{message}" }
            }
        }
    }
}

#[component]
fn SignalForm(slug: String, project: String, on_added: EventHandler<()>) -> Element {
    let mut base_url = use_signal(String::new);
    let mut number = use_signal(String::new);
    let mut token = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move |evt: FormEvent| {
        let (slug, project) = (slug.clone(), project.clone());
        async move {
            evt.prevent_default();
            if let Err(message) = validate_signal(&base_url(), &number(), &token()) {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match connect_signal(slug, project, base_url(), number(), token()).await {
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
            div { class: "conn-grid",
                div { class: "orgs-field",
                    Label { html_for: "sig-url", "Bridge URL" }
                    Input { id: "sig-url", placeholder: "https://signal.example.com", value: base_url(), oninput: move |e: FormEvent| base_url.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "sig-number", "Registered number" }
                    Input { id: "sig-number", placeholder: "+33 6 12 34 56 78", value: number(), oninput: move |e: FormEvent| number.set(e.value()) }
                }
                div { class: "orgs-field",
                    Label { html_for: "sig-token", "Bridge token" }
                    Input { id: "sig-token", r#type: "password", autocomplete: "off", value: token(), oninput: move |e: FormEvent| token.set(e.value()) }
                }
            }
            p { class: "conn-meta",
                "Signal has no official bot API: run signal-cli-rest-api (normal or native mode) with the number "
                "registered, behind a reverse proxy that requires this token as a Bearer token."
            }
            Button { r#type: "submit", disabled: busy(), "Connect Signal" }
            if let Some(message) = error() {
                p { class: "orgs-error", "{message}" }
            }
        }
    }
}

#[component]
fn MessageRow(message: Message, reply_to: Signal<Option<(String, String)>>) -> Element {
    let inbound = message.direction == "in";
    let who = message
        .peer_name
        .clone()
        .map(|n| format!("{n} ({})", message.peer))
        .unwrap_or_else(|| message.peer.clone());
    let arrow = if inbound { "from" } else { "to" };
    let (channel_id, peer) = (message.channel_id.clone(), message.peer.clone());
    let can_reply = inbound && message.provider != Provider::Slack;
    rsx! {
        div { class: if inbound { "inbox-msg in" } else { "inbox-msg out" },
            div { class: "conn-meta",
                "{message.provider.name()} · {message.channel_label} · {arrow} {who} · {message.created_at}"
            }
            div { class: "inbox-body", "{message.body}" }
            if can_reply {
                div { class: "conn-actions",
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| reply_to.set(Some((channel_id.clone(), peer.clone()))),
                        "Reply"
                    }
                }
            }
        }
    }
}

/// Send through an interface: to its channel (Slack), or to a number
/// (WhatsApp, Signal) — prefilled by "Reply".
#[component]
fn Composer(
    slug: String,
    project: String,
    channels: Vec<Channel>,
    reply_to: Signal<Option<(String, String)>>,
    on_sent: EventHandler<()>,
) -> Element {
    let first = channels.first().map(|c| c.id.clone()).unwrap_or_default();
    let mut channel = use_signal(|| first);
    let mut recipient = use_signal(String::new);
    let mut text = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    // "Reply" picks the channel and the number.
    use_effect(move || {
        if let Some((c, peer)) = reply_to() {
            channel.set(c);
            recipient.set(peer);
        }
    });

    let provider_of = {
        let channels = channels.clone();
        move |id: &str| channels.iter().find(|c| c.id == id).map(|c| c.provider)
    };
    let provider = provider_of(&channel());
    let needs_recipient = provider.is_some_and(|p| p != Provider::Slack);

    let submit = move |evt: FormEvent| {
        let (slug, project) = (slug.clone(), project.clone());
        async move {
            evt.prevent_default();
            let checked = validate_message(&text()).and_then(|_| {
                if needs_recipient {
                    validate_phone(&recipient()).map(|_| ())
                } else {
                    Ok(())
                }
            });
            if let Err(message) = checked {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            match send_message(slug, project, channel(), recipient(), text()).await {
                Ok(_) => {
                    text.set(String::new());
                    error.set(None);
                    on_sent.call(());
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };

    rsx! {
        form { class: "conn-section", onsubmit: submit,
            h4 { "Send a message" }
            div { class: "conn-picker",
                div { class: "conn-select", key: "{channel}",
                    Select::<String> {
                        default_value: channel(),
                        aria_label: "Interface",
                        on_value_change: move |v: Option<String>| {
                            if let Some(id) = v {
                                channel.set(id);
                            }
                        },
                        for (i, c) in channels.iter().enumerate() {
                            SelectOption::<String> {
                                key: "{c.id}",
                                index: i,
                                value: c.id.clone(),
                                text_value: format!("{} · {}", c.provider.name(), c.label),
                                "{c.provider.name()} · {c.label}"
                            }
                        }
                    }
                }
                if needs_recipient {
                    div { class: "orgs-field",
                        Label { html_for: "msg-to", "To" }
                        Input { id: "msg-to", placeholder: "+33 6 12 34 56 78", value: recipient(), oninput: move |e: FormEvent| recipient.set(e.value()) }
                    }
                }
            }
            div { class: "orgs-field conn-wide",
                Label { html_for: "msg-text", "Message" }
                Textarea {
                    id: "msg-text",
                    rows: 3,
                    value: text(),
                    oninput: move |e: FormEvent| text.set(e.value()),
                }
            }
            if provider == Some(Provider::Whatsapp) {
                p { class: "conn-meta", "WhatsApp only delivers free text within 24 hours of the person's last message." }
            }
            Button { r#type: "submit", disabled: busy(), "Send" }
            if let Some(message) = error() {
                p { class: "orgs-error", "{message}" }
            }
        }
    }
}
