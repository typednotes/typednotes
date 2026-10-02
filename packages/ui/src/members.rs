use api::{add_member, list_members, may_manage, remove_member, set_member_role, Member, ROLES};
use dioxus::prelude::*;

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::components::select::{use_selected, Select, SelectOption};
use crate::{error_message, navigate_to};

/// An org's members: owners and admins add people by email and remove
/// them; anyone may leave.
#[component]
pub(crate) fn MembersPanel(slug: ReadSignal<String>, role: String) -> Element {
    let mut members = use_server_future(move || list_members(slug()))?;
    let manages = may_manage(&role, "member");

    rsx! {
        Card {
            CardHeader {
                CardTitle { "Members" }
                CardDescription {
                    "Everyone here sees the org's projects, notebooks and connections. "
                    "Owners and admins add people by email."
                }
            }
            CardContent {
                match members() {
                    None => rsx! { p { "Loading…" } },
                    Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load members: {error_message(&e)}" } },
                    Some(Ok(list)) => rsx! {
                        div { class: "conn-list",
                            for member in list {
                                MemberRow {
                                    key: "{member.user_id}",
                                    slug: slug(),
                                     member,
                                    actor_role: role.clone(),
                                    on_removed: move |left: bool| {
                                        if left {
                                            navigate_to("/");
                                        } else {
                                            members.restart();
                                        }
                                    },
                                }
                            }
                        }
                    },
                }
                if manages {
                    AddMember { slug: slug(), role: role.clone(), on_added: move |_| members.restart() }
                }
            }
        }
    }
}

#[component]
fn MemberRow(slug: String, member: Member, actor_role: String, on_removed: EventHandler<bool>) -> Element {
    let mut confirming = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let who = member
        .display_name
        .clone()
        .unwrap_or_else(|| member.email.clone());
    let id = member.user_id.clone();
    let is_you = member.is_you;
    rsx! {
        div { class: "conn-row",
            div { class: "conn-main",
                span { class: "conn-provider", "{who}" }
                if member.display_name.is_some() {
                    span { class: "conn-label", "{member.email}" }
                }
                span { class: "conn-status conn-status-active", "{member.role}" }
                if is_you {
                    span { class: "conn-meta", "you" }
                }
            }
            div { class: "conn-meta",
                if member.signed_in { "added {member.added_at}" } else { "invited {member.added_at} · has not signed in yet" }
            }
            if member.can_remove {
                div { class: "conn-actions",
                    if actor_role == "owner" && member.role != "owner" && !is_you {
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, disabled: busy(),
                            title: "An owner can manage members and permissions and delete this organization.",
                            onclick: {
                                let (slug, id) = (slug.clone(), id.clone()); move |_| {
                                    let (slug, id) = (slug.clone(), id.clone()); async move {
                                        busy.set(true);
                                        match set_member_role(slug, id, "owner".into()).await {
                                            Ok(()) => { error.set(None); on_removed.call(false); },
                                            Err(e) => error.set(Some(error_message(&e))),
                                        }
                                        busy.set(false);
                                    }
                                }
                            }, "Make owner" }
                    }
                    Button {
                        size: ButtonSize::Sm,
                        disabled: busy(),
                        variant: if confirming() { ButtonVariant::Destructive } else { ButtonVariant::Ghost },
                        onclick: move |_| {
                            let (slug, id) = (slug.clone(), id.clone());
                            async move {
                                if !confirming() {
                                    confirming.set(true);
                                    return;
                                }
                                match remove_member(slug, id).await {
                                    Ok(()) => on_removed.call(is_you),
                                    Err(e) => {
                                        error.set(Some(error_message(&e)));
                                        confirming.set(false);
                                    }
                                }
                            }
                        },
                        match (confirming(), is_you) {
                            (false, true) => "Leave the org",
                            (false, false) => "Remove",
                            (true, true) => "Confirm: leave",
                            (true, false) => "Confirm removal",
                        }
                    }
                }
            }
            if let Some(e) = error() {
                div { class: "conn-result err", "{e}" }
            }
        }
    }
}

#[component]
fn AddMember(slug: String, role: String, on_added: EventHandler<()>) -> Element {
    let mut email = use_signal(String::new);
    let mut new_role = use_signal(|| "member".to_string());
    let selected_role = use_selected(new_role.into());
    let mut error = use_signal(|| None::<String>);
    let mut done = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let allowed: Vec<&'static str> = ROLES.into_iter().filter(|r| may_manage(&role, r)).collect();

    let submit = move |evt: FormEvent| {
        let slug = slug.clone();
        async move {
            evt.prevent_default();
            busy.set(true);
            match add_member(slug, email(), new_role()).await {
                Ok(m) => {
                    done.set(Some(format!("{} added as {}.", m.email, m.role)));
                    error.set(None);
                    email.set(String::new());
                    on_added.call(());
                }
                Err(e) => {
                    done.set(None);
                    error.set(Some(error_message(&e)));
                }
            }
            busy.set(false);
        }
    };

    rsx! {
        div { class: "conn-section",
            h4 { "Add a member" }
            form { class: "orgs-form", onsubmit: submit,
                div { class: "orgs-field",
                    Label { html_for: "member-email", "Email" }
                    Input {
                        id: "member-email",
                        r#type: "email",
                        placeholder: "ada@example.com",
                        value: email(),
                        oninput: move |e: FormEvent| email.set(e.value()),
                    }
                }
                div { class: "orgs-field conn-select",
                    Label { html_for: "member-role", "Role" }
                    Select::<String> {
                        id: "member-role", value: Some(selected_role), disabled: busy(),
                        aria_label: "Role",
                        on_value_change: move |v: Option<String>| {
                            if let Some(r) = v {
                                new_role.set(r);
                            }
                        },
                        for (i, r) in allowed.iter().enumerate() {
                            SelectOption::<String> { key: "{r}", index: i, value: r.to_string(), text_value: r.to_string(), "{r}" }
                        }
                    }
                }
                Button { r#type: "submit", disabled: busy() || email().trim().is_empty(), "Add" }
            }
            p { class: "conn-meta",
                "Someone who has not signed in yet gets access on their first sign-in with a verified "
                "GitHub or Google address that matches."
            }
            if let Some(m) = done() {
                p { class: "conn-result ok", "{m}" }
            }
            if let Some(e) = error() {
                p { class: "orgs-error", "{e}" }
            }
        }
    }
}
