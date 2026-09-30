//! Settings, in three places with one layout — a list of sections on the
//! left, the section on the right:
//!
//! | Page | Sections |
//! |---|---|
//! | `/settings` — your account | Profile · Sign-in · Sessions · Organisations |
//! | `/orgs/:org/settings/:section` | General · Members · Connections · Notebooks · Danger zone |
//! | `/orgs/:org/projects/:project/settings/:section` | General · Repository · Interfaces · Danger zone |
//!
//! The pages without `settings` in their path are for the work itself —
//! projects, notebooks, the inbox — and link here when something is missing.

use api::{
    current_user, delete_org, get_account, get_org, get_org_settings, get_project, remove_member,
    rename_org, rename_project, set_display_name, set_org_settings, sign_out_elsewhere, Org,
    OrgSettings, Provider, MAX_AUTO_REPAIRS, EffectPolicy, NOTEBOOK_EFFECTS, WRITER_TOOLS,
    set_notebook_permissions,
    ConnectorPermissions,
};
use dioxus::prelude::*;

use crate::auth::LoginPanel;
use crate::channels::InterfacesPanel;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::components::textarea::Textarea;
use crate::connections::{ConnectionsPanel, PermissionEditor, CONNECTIONS_CSS};
use crate::members::MembersPanel;
use crate::orgs::ORGS_CSS;
use crate::projects::{DeleteProject, RepoPanel};
use crate::{error_message, navigate_to};

const SETTINGS_CSS: Asset = asset!("/assets/styling/settings.css");

/// An org's settings sections.
pub const ORG_SECTIONS: [(&str, &str); 5] = [
    ("general", "General"),
    ("members", "Members"),
    ("connections", "Connections"),
    ("notebooks", "Notebooks"),
    ("danger", "Danger zone"),
];

/// A project's settings sections.
pub const PROJECT_SECTIONS: [(&str, &str); 4] = [
    ("general", "General"),
    ("repository", "Repository"),
    ("interfaces", "Interfaces"),
    ("danger", "Danger zone"),
];

/// A known section, else the first.
fn section_of(sections: &[(&'static str, &'static str)], id: &str) -> &'static str {
    sections
        .iter()
        .find(|(s, _)| *s == id)
        .map(|(s, _)| *s)
        .unwrap_or(sections[0].0)
}

/// The shared frame: a back link, the title, the section list and the
/// section. `base` is the settings path the sections hang off.
#[component]
fn SettingsLayout(
    back_href: String,
    back_label: String,
    title: String,
    subtitle: Option<String>,
    base: String,
    sections: Vec<(&'static str, &'static str)>,
    active: String,
    children: Element,
) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: ORGS_CSS }
        document::Link { rel: "stylesheet", href: CONNECTIONS_CSS }
        document::Link { rel: "stylesheet", href: SETTINGS_CSS }
        div { class: "settings",
            p { class: "back-link", a { href: "{back_href}", "← {back_label}" } }
            header { class: "settings-head",
                h2 { class: "settings-title", "{title}" }
                if let Some(s) = subtitle {
                    p { class: "conn-meta", "{s}" }
                }
            }
            div { class: "settings-frame",
                if !sections.is_empty() {
                    nav { class: "settings-nav", aria_label: "Settings sections",
                        for (id, label) in sections.iter() {
                            a {
                                key: "{id}",
                                class: if *id == active { "settings-link active" } else { "settings-link" },
                                href: "{base}/{id}",
                                aria_current: if *id == active { "page" } else { "false" },
                                "{label}"
                            }
                        }
                    }
                }
                div { class: "settings-body", {children} }
            }
        }
    }
}

/// A name, and a button to save it, for renaming an org or a project.
#[component]
fn NameForm(
    id: String,
    initial: String,
    can_edit: bool,
    on_save: EventHandler<String>,
    busy: bool,
) -> Element {
    let mut name = use_signal(|| initial.clone());
    let changed = name().trim() != initial.trim() && !name().trim().is_empty();
    rsx! {
        form {
            class: "conn-picker",
            onsubmit: move |e: FormEvent| {
                e.prevent_default();
                on_save.call(name());
            },
            div { class: "orgs-field",
                Label { html_for: "{id}", "Name" }
                Input { id: "{id}", disabled: !can_edit, value: name(), oninput: move |e: FormEvent| name.set(e.value()) }
            }
            if can_edit {
                Button { r#type: "submit", disabled: busy || !changed, "Save" }
            }
        }
    }
}

/// Saved / failed, under a form.
#[component]
fn Outcome(status: Option<Result<String, String>>) -> Element {
    match status {
        Some(Ok(m)) => rsx! { p { class: "conn-result ok", "{m}" } },
        Some(Err(e)) => rsx! { p { class: "orgs-error", "{e}" } },
        None => rsx! {},
    }
}

/// The query-string flash an OAuth round trip comes back with.
#[component]
fn Flash(connected: String, error: String) -> Element {
    let done = Provider::from_id(&connected).map(|p| format!("{} connected.", p.name()));
    rsx! {
        if let Some(m) = done {
            p { class: "orgs-status ok", "{m}" }
        }
        if !error.is_empty() {
            p { class: "orgs-error", "{error}" }
        }
    }
}

// ── Your account ────────────────────────────────────────────────────────

/// `/settings`: the signed-in user's profile, sign-in methods, sessions and
/// organisations — one page, it is short.
#[component]
pub fn AccountPage() -> Element {
    let mut account = use_server_future(get_account)?;
    let mut name_status = use_signal(|| None::<Result<String, String>>);
    let mut session_status = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);

    let a = match account() {
        None => return rsx! { p { "Loading…" } },
        Some(Err(ServerFnError::ServerError { code: 401, .. })) => {
            return rsx! { LoginPanel { error: None } }
        }
        Some(Err(e)) => {
            return rsx! { p { class: "orgs-error", "Could not load your account: {error_message(&e)}" } }
        }
        Some(Ok(a)) => a,
    };
    let me = a.user.id.clone();

    rsx! {
        SettingsLayout {
            back_href: "/",
            back_label: "Your organisations",
            title: "Your account",
            subtitle: Some(a.user.email.clone()),
            base: "/settings",
            sections: Vec::new(),
            active: "",
            Card {
                CardHeader {
                    CardTitle { "Profile" }
                    CardDescription { "How the people you work with see you. Your email is the one your sign-in provider verified." }
                }
                CardContent {
                    NameForm {
                        id: "account-name",
                        initial: a.user.display_name.clone().unwrap_or_default(),
                        can_edit: true,
                        busy: busy(),
                        on_save: move |name: String| async move {
                            busy.set(true);
                            match set_display_name(name).await {
                                Ok(_) => {
                                    name_status.set(Some(Ok("Saved.".into())));
                                    account.restart();
                                }
                                Err(e) => name_status.set(Some(Err(error_message(&e)))),
                            }
                            busy.set(false);
                        },
                    }
                    p { class: "conn-meta", "Email: {a.user.email}" }
                    Outcome { status: name_status() }
                }
            }
            Card {
                CardHeader {
                    CardTitle { "Sign-in" }
                    CardDescription {
                        "Signing in with GitHub or Google under the same verified email is the same account: "
                        "sign in with the other one to add it."
                    }
                }
                CardContent {
                    div { class: "conn-list",
                        for i in a.identities.iter() {
                            div { class: "conn-row", key: "{i.provider}{i.created_at}",
                                div { class: "conn-main",
                                    span { class: "conn-provider", "{i.provider}" }
                                    span { class: "conn-meta",
                                        match &i.last_login_at {
                                            Some(t) => format!("last used {t}"),
                                            None => format!("linked {}", i.created_at),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Card {
                CardHeader {
                    CardTitle { "Sessions" }
                    CardDescription { "Browsers signed in to your account. Sessions last 30 days." }
                }
                CardContent {
                    div { class: "conn-picker",
                        p { class: "settings-inline",
                            if a.sessions == 1 { "Only this browser is signed in." } else { "{a.sessions} browsers are signed in, this one included." }
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            disabled: busy() || a.sessions <= 1,
                            onclick: move |_| async move {
                                busy.set(true);
                                match sign_out_elsewhere().await {
                                    Ok(n) => {
                                        session_status.set(Some(Ok(format!("Signed out {n} other browser{}.", if n == 1 { "" } else { "s" }))));
                                        account.restart();
                                    }
                                    Err(e) => session_status.set(Some(Err(error_message(&e)))),
                                }
                                busy.set(false);
                            },
                            "Sign out everywhere else"
                        }
                    }
                    Outcome { status: session_status() }
                }
            }
            Card {
                CardHeader {
                    CardTitle { "Organisations" }
                    CardDescription { "The orgs you belong to, and your role in each." }
                }
                CardContent {
                    if a.orgs.is_empty() {
                        p { class: "orgs-empty", "None yet: create one from ", a { href: "/", "your organisations" }, "." }
                    }
                    div { class: "conn-list",
                        for org in a.orgs.iter() {
                            AccountOrgRow { key: "{org.id}", org: org.clone(), me: me.clone(), on_left: move |_| account.restart() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AccountOrgRow(org: Org, me: String, on_left: EventHandler<()>) -> Element {
    let mut confirming = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let slug = org.slug.clone();
    rsx! {
        div { class: "conn-row",
            div { class: "conn-main",
                a { class: "conn-provider", href: "/orgs/{org.slug}", "{org.name}" }
                span { class: "conn-status conn-status-active", "{org.role}" }
                a { class: "conn-meta", href: "/orgs/{org.slug}/settings", "settings" }
            }
            div { class: "conn-actions",
                Button {
                    size: ButtonSize::Sm,
                    variant: if confirming() { ButtonVariant::Destructive } else { ButtonVariant::Ghost },
                    onclick: move |_| {
                        let (slug, me) = (slug.clone(), me.clone());
                        async move {
                            if !confirming() {
                                confirming.set(true);
                                return;
                            }
                            match remove_member(slug, me).await {
                                Ok(()) => on_left.call(()),
                                Err(e) => {
                                    error.set(Some(error_message(&e)));
                                    confirming.set(false);
                                }
                            }
                        }
                    },
                    if confirming() { "Confirm: leave {org.name}" } else { "Leave" }
                }
            }
            if let Some(e) = error() {
                div { class: "conn-result err", "{e}" }
            }
        }
    }
}

// ── An org ──────────────────────────────────────────────────────────────

/// `/orgs/:org/settings/:section`.
#[component]
pub fn OrgSettingsPage(
    slug: ReadSignal<String>,
    section: String,
    connected: String,
    error: String,
) -> Element {
    let me = use_server_future(current_user)?;
    let mut detail = use_server_future(move || get_org(slug()))?;
    if let Some(Ok(None)) = me() {
        return rsx! { LoginPanel { error: None } };
    }
    let d = match detail() {
        None => return rsx! { p { "Loading…" } },
        Some(Err(e)) => {
            return rsx! { p { class: "orgs-error", "Could not load this organisation: {error_message(&e)}" } }
        }
        Some(Ok(d)) => d,
    };
    let active = section_of(&ORG_SECTIONS, &section);
    let org = d.org.clone();
    let admin = org.role == "owner" || org.role == "admin";

    rsx! {
        SettingsLayout {
            back_href: "/orgs/{org.slug}",
            back_label: "{org.name}",
            title: "Settings",
            subtitle: Some(format!("{} · you are {}", org.name, org.role)),
            base: "/orgs/{org.slug}/settings",
            sections: ORG_SECTIONS.to_vec(),
            active: active.to_string(),
            Flash { connected, error }
            match active {
                "members" => rsx! { MembersPanel { slug: org.slug.clone(), role: org.role.clone() } },
                "connections" => rsx! { ConnectionsPanel { slug: org.slug.clone() } },
                "notebooks" => rsx! { OrgNotebooks { slug: org.slug.clone() } },
                "danger" => rsx! { OrgDanger { org: org.clone(), me: me().and_then(|u| u.ok()).flatten().map(|u| u.id).unwrap_or_default() } },
                _ => rsx! {
                    OrgGeneral { org: org.clone(), credits: d.credits, admin, on_saved: move |_| detail.restart() }
                },
            }
        }
    }
}

#[component]
fn OrgGeneral(org: Org, credits: Option<i64>, admin: bool, on_saved: EventHandler<()>) -> Element {
    let mut status = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);
    let slug = org.slug.clone();
    rsx! {
        Card {
            CardHeader {
                CardTitle { "General" }
                CardDescription { "The org's name is what members see; its slug, " code { "{org.slug}" } ", is its address and does not change." }
            }
            CardContent {
                NameForm {
                    id: "org-name",
                    initial: org.name.clone(),
                    can_edit: admin,
                    busy: busy(),
                    on_save: move |name: String| {
                        let slug = slug.clone();
                        async move {
                            busy.set(true);
                            match rename_org(slug, name).await {
                                Ok(_) => {
                                    status.set(Some(Ok("Saved.".into())));
                                    on_saved.call(());
                                }
                                Err(e) => status.set(Some(Err(error_message(&e)))),
                            }
                            busy.set(false);
                        }
                    },
                }
                if !admin {
                    p { class: "conn-meta", "Only the org's owners and admins can rename it." }
                }
                Outcome { status: status() }
                dl { class: "settings-facts",
                    dt { "Created" } dd { "{org.created_at}" }
                    dt { "Credits" }
                    dd {
                        match credits {
                            Some(c) => format!("{c} available"),
                            None => "unavailable".to_string(),
                        }
                    }
                }
            }
        }
    }
}

/// The notebooks' settings of an org: the cap on automatic rewrites.
#[component]
fn OrgNotebooks(slug: ReadSignal<String>) -> Element {
    let mut settings = use_server_future(move || get_org_settings(slug()))?;
    match settings() {
        None => rsx! { p { "Loading…" } },
        Some(Err(e)) => {
            rsx! { p { class: "orgs-error", "Could not load the settings: {error_message(&e)}" } }
        }
        Some(Ok(s)) => {
            rsx! {
                NotebookPermissions { key: "{s.effect_policy:?}", slug: slug(), policy: s.effect_policy.clone(), can_edit: s.can_edit, on_saved: move |_| settings.restart() }
                AutoRepairs { slug: slug(), settings: s, on_saved: move |_| settings.restart() }
            }
        }
    }
}

#[component]
fn NotebookPermissions(slug: String, policy: EffectPolicy, can_edit: bool, on_saved: EventHandler<()>) -> Element {
    let mut effects = use_signal(|| policy.effects.clone());
    let mut tools = use_signal(|| policy.tools.clone());
    let mut providers = use_signal(|| policy.providers.clone());
    let mut domains = use_signal(|| policy.domains.join("\n"));
    let mut configured_domains = use_signal(|| policy.configured_domains);
    let mut connector_ceilings = use_signal(|| policy.connector_ceilings.clone());
    let mut busy = use_signal(|| false);
    let mut status = use_signal(|| None::<Result<String, String>>);
    let save = move |event: FormEvent| {
        event.prevent_default();
        let slug = slug.clone();
        let connector_ceilings = connector_ceilings();
        async move {
            busy.set(true);
            let policy = EffectPolicy { effects: effects(), tools: tools(), providers: providers(), configured_domains: configured_domains(), domains: domains().split([',', '\n']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect(), connector_ceilings };
            match set_notebook_permissions(slug, policy).await {
                Ok(_) => { status.set(Some(Ok("Notebook permissions saved.".into()))); on_saved.call(()); }
                Err(error) => status.set(Some(Err(error_message(&error)))),
            }
            busy.set(false);
        }
    };
    rsx! {
        Card {
            CardHeader {
                CardTitle { "Notebook permissions" }
                CardDescription { "Organization-owned upper bounds for generated code. Cells cannot grant themselves additional access." }
            }
            CardContent {
                form { class: "conn-subform", onsubmit: save,
                    h4 { "Effects" }
                    div { class: "conn-actions",
                        for name in NOTEBOOK_EFFECTS {
                            Button { r#type: "button", size: ButtonSize::Sm, disabled: !can_edit || busy(),
                                variant: if effects().iter().any(|value| value == name) { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                                aria_pressed: effects().iter().any(|value| value == name).to_string(),
                                onclick: move |_| effects.with_mut(|values| { if let Some(i) = values.iter().position(|value| value == name) { values.remove(i); } else { values.push(name.to_string()); } }), "{name}" }
                        }
                    }
                    div { class: "orgs-field",
                        Button { r#type: "button", variant: ButtonVariant::Outline, disabled: !can_edit, aria_pressed: configured_domains().to_string(), onclick: move |_| configured_domains.set(!configured_domains()),
                            if configured_domains() { "Domains: use each cell's configured URL" } else { "Domains: use an organization allowlist" }
                        }
                        Label { html_for: "notebook-domains", "Allowed HTTP domains (one per line)" }
                        Textarea { id: "notebook-domains", rows: 3, value: domains(), disabled: !can_edit || configured_domains(),
                            placeholder: "api.example.com\nexample.com", oninput: move |event: FormEvent| domains.set(event.value()) }
                        p { class: "conn-meta", "Exact public hostnames; empty denies network requests. Subdomains must be listed separately." }
                    }
                    p { class: "conn-meta", "Database access is restricted to the running user's schema. Files are restricted to that organization and user's temporary folder. These boundaries cannot be widened here." }
                    details {
                        summary { "Code-writing tools" }
                        div { class: "conn-actions",
                            for name in WRITER_TOOLS {
                                Button { r#type: "button", size: ButtonSize::Sm, disabled: !can_edit || busy(),
                                    variant: if tools().iter().any(|value| value == name) { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                                    aria_pressed: tools().iter().any(|value| value == name).to_string(),
                                    onclick: move |_| tools.with_mut(|values| { if let Some(i) = values.iter().position(|value| value == name) { values.remove(i); } else { values.push(name.to_string()); } }), "{name}" }
                            }
                        }
                    }
                    details {
                        summary { "Connector providers" }
                        div { class: "conn-actions",
                            for &provider in Provider::ALL.iter() {
                                Button { r#type: "button", size: ButtonSize::Sm, disabled: !can_edit || busy(),
                                    variant: if providers().iter().any(|value| value == provider.id()) { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                                    aria_pressed: providers().iter().any(|value| value == provider.id()).to_string(),
                                    onclick: move |_| providers.with_mut(|values| { if let Some(i) = values.iter().position(|value| value == provider.id()) { values.remove(i); } else { values.push(provider.id().to_string()); } }), "{provider.name()}" }
                            }
                        }
                    }
                    details { class: "permission-advanced",
                        summary { "Advanced connector ceilings" }
                        p { class: "conn-meta", "Optional limits for every connection of a provider. Inherit keeps each connection's ceiling; no JSON is needed. Disabling a provider above denies it entirely." }
                        for &provider in Provider::ALL.iter() {
                            details { class: "provider-ceiling", key: "{provider.id()}",
                                summary { "{provider.name()}" }
                                PermissionEditor { id: "org-{provider.id()}", provider, value: connector_ceilings().get(provider.id()).cloned(), ceiling: None,
                                    disabled: !can_edit || busy(),
                                    on_change: move |value: Option<ConnectorPermissions>| connector_ceilings.with_mut(|ceilings| {
                                        if let Some(value) = value { ceilings.insert(provider.id().into(), value); } else { ceilings.remove(provider.id()); }
                                    }) }
                            }
                        }
                    }
                    if can_edit { Button { r#type: "submit", disabled: busy(), "Save permissions" } }
                    Outcome { status: status() }
                }
            }
        }
    }
}

/// The cap on rewrites of a notebook's code Typednotes starts by itself.
#[component]
fn AutoRepairs(slug: String, settings: OrgSettings, on_saved: EventHandler<()>) -> Element {
    let current = settings
        .auto_repairs
        .unwrap_or(settings.default_auto_repairs);
    let mut value = use_signal(|| current.to_string());
    let mut status = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);

    let save = {
        let slug = slug.clone();
        move |cap: Option<i32>| {
            let slug = slug.clone();
            async move {
                busy.set(true);
                match set_org_settings(slug, cap).await {
                    Ok(s) => {
                        value.set(s.auto_repairs.unwrap_or(s.default_auto_repairs).to_string());
                        status.set(Some(Ok("Saved.".to_string())));
                        on_saved.call(());
                    }
                    Err(e) => status.set(Some(Err(error_message(&e)))),
                }
                busy.set(false);
            }
        }
    };
    let (save_value, reset) = (save.clone(), save);
    let parsed = value()
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|n| (0..=MAX_AUTO_REPAIRS).contains(n));

    rsx! {
        Card {
            CardHeader {
                CardTitle { "Automatic rewrites" }
                CardDescription {
                    "When a cell's code does not build, or fails when it runs, Typednotes rewrites it "
                    "by itself — each rewrite spends the org's credits on AI model calls. This caps how "
                    "many rewrites it starts on its own before one of you asks for one again."
                }
            }
            CardContent {
                div { class: "conn-picker",
                    div { class: "orgs-field settings-number",
                        Label { html_for: "auto-repairs", "Rewrites in a row (0 to {MAX_AUTO_REPAIRS})" }
                        Input {
                            id: "auto-repairs",
                            r#type: "number",
                            min: "0",
                            max: "{MAX_AUTO_REPAIRS}",
                            step: "1",
                            disabled: !settings.can_edit,
                            value: value(),
                            oninput: move |e: FormEvent| value.set(e.value()),
                        }
                    }
                    if settings.can_edit {
                        Button {
                            disabled: busy() || parsed.is_none(),
                            onclick: move |_| save_value(parsed),
                            "Save"
                        }
                        if settings.auto_repairs.is_some() {
                            Button {
                                variant: ButtonVariant::Ghost,
                                disabled: busy(),
                                onclick: move |_| reset(None),
                                "Use the default ({settings.default_auto_repairs})"
                            }
                        }
                    }
                }
                p { class: "conn-meta",
                    match (current, settings.auto_repairs.is_some()) {
                        (0, _) => "Off: a failing cell waits for one of you to ask for a fix.".to_string(),
                        (n, true) => format!("Up to {n} automatic rewrite{} in a row.", if n == 1 { "" } else { "s" }),
                        (n, false) => format!("Up to {n} automatic rewrite{} in a row (the default).", if n == 1 { "" } else { "s" }),
                    }
                }
                if !settings.can_edit {
                    p { class: "conn-meta", "Only the org's owners and admins can change this." }
                }
                Outcome { status: status() }
            }
        }
    }
}

/// Leaving the org, and — for owners — deleting it.
#[component]
fn OrgDanger(org: Org, me: String) -> Element {
    let mut confirm = use_signal(String::new);
    let mut leaving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let owner = org.role == "owner";
    let (slug, slug2) = (org.slug.clone(), org.slug.clone());
    rsx! {
        Card {
            CardHeader {
                CardTitle { "Leave the organisation" }
                CardDescription { "You lose access to its projects, notebooks and connections. An org keeps at least one owner." }
            }
            CardContent {
                Button {
                    variant: if leaving() { ButtonVariant::Destructive } else { ButtonVariant::Outline },
                    disabled: busy(),
                    onclick: move |_| {
                        let (slug, me) = (slug.clone(), me.clone());
                        async move {
                            if !leaving() {
                                leaving.set(true);
                                return;
                            }
                            busy.set(true);
                            match remove_member(slug, me).await {
                                Ok(()) => navigate_to("/"),
                                Err(e) => {
                                    error.set(Some(error_message(&e)));
                                    leaving.set(false);
                                }
                            }
                            busy.set(false);
                        }
                    },
                    if leaving() { "Confirm: leave {org.name}" } else { "Leave {org.name}" }
                }
            }
        }
        if owner {
            Card {
                CardHeader {
                    CardTitle { "Delete the organisation" }
                    CardDescription {
                        "Deletes its projects, notebooks and messages, removes every connection's credential and "
                        "notebook secret, and drops its members' notebook databases. This cannot be undone."
                    }
                }
                CardContent {
                    div { class: "conn-picker",
                        div { class: "orgs-field",
                            Label { html_for: "delete-org", "Type {org.slug} to confirm" }
                            Input { id: "delete-org", autocomplete: "off", value: confirm(), oninput: move |e: FormEvent| confirm.set(e.value()) }
                        }
                        Button {
                            variant: ButtonVariant::Destructive,
                            disabled: busy() || confirm().trim() != org.slug,
                            onclick: move |_| {
                                let slug = slug2.clone();
                                async move {
                                    busy.set(true);
                                    match delete_org(slug, confirm()).await {
                                        Ok(()) => navigate_to("/"),
                                        Err(e) => error.set(Some(error_message(&e))),
                                    }
                                    busy.set(false);
                                }
                            },
                            "Delete the organisation"
                        }
                    }
                }
            }
        }
        if let Some(e) = error() {
            p { class: "orgs-error", "{e}" }
        }
    }
}

// ── A project ───────────────────────────────────────────────────────────

/// `/orgs/:org/projects/:project/settings/:section`.
#[component]
pub fn ProjectSettingsPage(
    slug: ReadSignal<String>,
    project: ReadSignal<String>,
    section: String,
    connected: String,
    error: String,
) -> Element {
    let me = use_server_future(current_user)?;
    let mut detail = use_server_future(move || get_project(slug(), project()))?;
    if let Some(Ok(None)) = me() {
        return rsx! { LoginPanel { error: None } };
    }
    let d = match detail() {
        None => return rsx! { p { "Loading…" } },
        Some(Err(e)) => {
            return rsx! { p { class: "orgs-error", "Could not load this project: {error_message(&e)}" } }
        }
        Some(Ok(d)) => d,
    };
    let active = section_of(&PROJECT_SECTIONS, &section);
    let (o, p) = (d.org.slug.clone(), d.project.slug.clone());

    rsx! {
        SettingsLayout {
            back_href: "/orgs/{o}/projects/{p}",
            back_label: "{d.project.name}",
            title: "Project settings",
            subtitle: Some(format!("{} · in {}", d.project.name, d.org.name)),
            base: "/orgs/{o}/projects/{p}/settings",
            sections: PROJECT_SECTIONS.to_vec(),
            active: active.to_string(),
            Flash { connected, error }
            match active {
                "repository" => rsx! {
                    RepoPanel { slug: o.clone(), project: d.project.clone(), on_changed: move |_| detail.restart() }
                },
                "interfaces" => rsx! { InterfacesPanel { slug: o.clone(), project: p.clone() } },
                "danger" => rsx! {
                    Card {
                        CardHeader {
                            CardTitle { "Delete the project" }
                            CardDescription { "Deletes its notebooks, interfaces and messages. Its repository is not touched." }
                        }
                        CardContent { DeleteProject { slug: o.clone(), project: p.clone() } }
                    }
                },
                _ => rsx! {
                    ProjectGeneral { org: o.clone(), project: d.project.clone(), on_saved: move |_| detail.restart() }
                },
            }
        }
    }
}

#[component]
fn ProjectGeneral(org: String, project: api::Project, on_saved: EventHandler<()>) -> Element {
    let mut status = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);
    let slug = project.slug.clone();
    let repository = format!("/orgs/{org}/projects/{}/settings/repository", project.slug);
    rsx! {
        Card {
            CardHeader {
                CardTitle { "General" }
                CardDescription { "Its slug, " code { "{project.slug}" } ", is its address and does not change." }
            }
            CardContent {
                NameForm {
                    id: "project-name",
                    initial: project.name.clone(),
                    can_edit: true,
                    busy: busy(),
                    on_save: move |name: String| {
                        let (org, slug) = (org.clone(), slug.clone());
                        async move {
                            busy.set(true);
                            match rename_project(org, slug, name).await {
                                Ok(_) => {
                                    status.set(Some(Ok("Saved.".into())));
                                    on_saved.call(());
                                }
                                Err(e) => status.set(Some(Err(error_message(&e)))),
                            }
                            busy.set(false);
                        }
                    },
                }
                Outcome { status: status() }
                dl { class: "settings-facts",
                    dt { "Created" } dd { "{project.created_at}" }
                    dt { "Repository" }
                    dd {
                        match &project.repo {
                            Some(r) => rsx! { a { href: "{r.web_url}", target: "_blank", rel: "noopener", "{r.full_name}" } },
                            None => rsx! { "none — " a { href: "{repository}", "choose one" } },
                        }
                    }
                }
            }
        }
    }
}
