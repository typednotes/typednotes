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
use crate::permission_check::PermissionCheck;

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
            p { class: "back-link", Link { to: "{back_href}", "← {back_label}" } }
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
                            Link {
                                key: "{id}",
                                class: if *id == active { "settings-link active" } else { "settings-link" },
                                to: "{base}/{id}",
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
                        p { class: "orgs-empty", "None yet: create one from ", Link { to: "/", "your organisations" }, "." }
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
                Link { class: "conn-provider", to: "/orgs/{org.slug}", "{org.name}" }
                span { class: "conn-status conn-status-active", "{org.role}" }
                Link { class: "conn-meta", to: "/orgs/{org.slug}/settings", "settings" }
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
                "connections" => rsx! { ConnectionsPanel { key: "{org.id}", slug: org.slug.clone() } },
                "notebooks" => rsx! { OrgNotebooks { key: "{org.id}", slug: org.slug.clone() } },
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
    let mut settings = use_resource(move || get_org_settings(slug()));
    match settings() {
        None => rsx! { p { "Loading…" } },
        Some(Err(e)) => {
            rsx! { p { class: "orgs-error", "Could not load the settings: {error_message(&e)}" } }
        }
        Some(Ok(s)) => {
            rsx! {
                NotebookPermissions { slug: slug(), policy: s.effect_policy.clone(), can_edit: s.can_edit, on_saved: move |_| settings.restart() }
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
    let mut toml_mode = use_signal(|| false);
    let mut toml_text = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut status = use_signal(|| None::<Result<String, String>>);
    let draft = move || EffectPolicy { effects: effects(), tools: tools(), providers: providers(), configured_domains: configured_domains(), domains: domains().split([',', '\n']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect(), connector_ceilings: connector_ceilings() };
    let switch_editor = move |_| {
        status.set(None);
        if toml_mode() {
            match EffectPolicy::from_toml(&toml_text()) {
                Ok(policy) => {
                    effects.set(policy.effects); tools.set(policy.tools); providers.set(policy.providers);
                    configured_domains.set(policy.configured_domains); domains.set(policy.domains.join("\n"));
                    connector_ceilings.set(policy.connector_ceilings); toml_mode.set(false);
                }
                Err(message) => status.set(Some(Err(message))),
            }
        } else {
            match draft().to_toml() {
                Ok(text) => { toml_text.set(text); toml_mode.set(true); }
                Err(message) => status.set(Some(Err(message))),
            }
        }
    };
    let save = move |event: FormEvent| {
        event.prevent_default();
        let slug = slug.clone();
        let parsed = if toml_mode() { EffectPolicy::from_toml(&toml_text()) } else { draft().validate() };
        async move {
            let policy = match parsed { Ok(policy) => policy, Err(message) => { status.set(Some(Err(message))); return; } };
            busy.set(true);
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
                    Button { r#type: "button", variant: ButtonVariant::Outline, disabled: busy(), onclick: switch_editor,
                        if toml_mode() { "Use visual editor" } else { "Edit as TOML" }
                    }
                    if toml_mode() {
                        div { class: "orgs-field conn-wide",
                            Label { html_for: "notebook-policy-toml", "Notebook permission policy (TOML)" }
                            Textarea { id: "notebook-policy-toml", class: "permission-toml", rows: 18,
                                disabled: !can_edit || busy(), value: toml_text(), oninput: move |event: FormEvent| toml_text.set(event.value()) }
                            p { class: "conn-meta", "Uses the same operation/resource schema as the visual editor. Unknown fields or operations are rejected. Changes apply only after Save permissions." }
                        }
                    } else {
                    h4 { "Effects" }
                    div { class: "permission-options",
                        for name in NOTEBOOK_EFFECTS {
                            PermissionCheck { id: "effect-{name}", label: *name, disabled: !can_edit || busy(),
                                checked: effects().iter().any(|value| value == name),
                                on_change: move |_: bool| effects.with_mut(|values| { if let Some(i) = values.iter().position(|value| value == name) { values.remove(i); } else { values.push(name.to_string()); } }) }
                        }
                    }
                    if !effects().iter().any(|effect| effect == "Connector") {
                        p { class: "conn-result err", "Connector is unchecked: AI, repository, calendar, email and messaging calls are denied, including connection tests. Enable it and save to allow the selected providers within their connection grants." }
                    }
                    p { class: "conn-meta", "Trace: logs · Error: failures · HTTP: public web requests · FileSystem: temporary files · PostgreSQL: notebook database · SecretStore: vault secrets · ObjectStore: S3/Azure · Connector: connected providers." }
                    div { class: "orgs-field",
                        fieldset { class: "permission-domain-mode",
                            legend { "HTTP domain policy" }
                            for (configured, label) in [(true, "Use each cell's configured URL"), (false, "Use an organization allowlist")] {
                                div { class: "permission-check",
                                    Input { id: "domain-mode-{configured}", name: "domain-mode", r#type: "radio", checked: configured_domains() == configured,
                                        disabled: !can_edit || busy(), oninput: move |_: FormEvent| configured_domains.set(configured) }
                                    Label { html_for: "domain-mode-{configured}", "{label}" }
                                }
                            }
                        }
                        Label { html_for: "notebook-domains", "Allowed HTTP domains (one per line)" }
                        Textarea { id: "notebook-domains", rows: 3, value: domains(), disabled: !can_edit || configured_domains(),
                            placeholder: "api.example.com\nexample.com", oninput: move |event: FormEvent| domains.set(event.value()) }
                        p { class: "conn-meta", "Exact public hostnames; empty denies network requests. Subdomains must be listed separately." }
                    }
                    p { class: "conn-meta", "Database access is restricted to the running user's schema. Files are restricted to that organization and user's temporary folder. These boundaries cannot be widened here." }
                    if tools().is_empty() {
                        p { class: "conn-result err", "No code-writing tools are enabled. Select the tools you intend to allow below and save before generating notebook code." }
                    }
                    details {
                        summary { "Code-writing tools" }
                        div { class: "permission-options",
                            for name in WRITER_TOOLS {
                                PermissionCheck { id: "tool-{name}", label: *name, disabled: !can_edit || busy(),
                                    checked: tools().iter().any(|value| value == name),
                                    on_change: move |_: bool| tools.with_mut(|values| { if let Some(i) = values.iter().position(|value| value == name) { values.remove(i); } else { values.push(name.to_string()); } }) }
                            }
                        }
                        p { class: "conn-meta", "Generating code normally needs read, ls, grep, write, edit, check, lsp, publish and lun_build. lun_call enables scoped runtime trials; bash is an independent grant." }
                    }
                    details {
                        summary { "Connector providers" }
                        div { class: "permission-options",
                            for &provider in Provider::ALL.iter() {
                                PermissionCheck { id: "provider-{provider.id()}", label: provider.name(), disabled: !can_edit || busy(),
                                    checked: providers().iter().any(|value| value == provider.id()),
                                    on_change: move |_: bool| providers.with_mut(|values| { if let Some(i) = values.iter().position(|value| value == provider.id()) { values.remove(i); } else { values.push(provider.id().to_string()); } }) }
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
                            None => rsx! { "none — " Link { to: "{repository}", "choose one" } },
                        }
                    }
                }
            }
        }
    }
}
