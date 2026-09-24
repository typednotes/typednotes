use api::{
    check_org_slug, check_project_slug, create_org, create_project, slugify, validate_org,
    validate_slug, SlugCheck,
};
use dioxus::prelude::*;

use crate::components::button::Button;
use crate::components::input::Input;
use crate::components::label::Label;
use crate::error_message;

/// What a [`NewSlugForm`] creates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Scope {
    Org,
    /// A project of the org with this slug.
    Project {
        org: String,
    },
}

/// A name and a slug, and a button to create the org or project.
///
/// The slug follows the name (`slugify`) until the user edits it, and is
/// checked while typing — its format here, its availability on the server —
/// so a taken or malformed slug is refused before submitting rather than by
/// a failed create. The server still enforces both; a create that loses a
/// race is reported like any other error.
#[component]
pub(crate) fn NewSlugForm(
    scope: Scope,
    name_placeholder: String,
    slug_placeholder: String,
    on_created: EventHandler<String>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut slug = use_signal(String::new);
    let mut slug_edited = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let check_scope = scope.clone();
    // Re-runs (cancelling the previous request) whenever the slug changes.
    let check = use_resource(move || {
        let scope = check_scope.clone();
        async move {
            let s = slug().trim().to_lowercase();
            if s.is_empty() {
                return None;
            }
            if let Err(message) = validate_slug(&s) {
                return Some(SlugCheck {
                    available: false,
                    message,
                });
            }
            let answer = match scope {
                Scope::Org => check_org_slug(s).await,
                Scope::Project { org } => check_project_slug(org, s).await,
            };
            Some(answer.unwrap_or_else(|e| SlugCheck {
                available: false,
                message: format!("could not check the slug: {}", error_message(&e)),
            }))
        }
    });
    let verdict = check().flatten();
    let blocked = verdict.as_ref().is_some_and(|c| !c.available);

    let submit = move |evt: FormEvent| {
        let scope = scope.clone();
        async move {
            evt.prevent_default();
            let (s, n) = (slug().trim().to_lowercase(), name().trim().to_string());
            if let Err(message) = validate_org(&s, &n) {
                error.set(Some(message));
                return;
            }
            busy.set(true);
            let created = match scope {
                Scope::Org => create_org(s, n).await.map(|o| o.slug),
                Scope::Project { org } => create_project(org, s, n).await.map(|p| p.slug),
            };
            match created {
                Ok(created) => {
                    slug.set(String::new());
                    name.set(String::new());
                    slug_edited.set(false);
                    error.set(None);
                    on_created.call(created);
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };

    rsx! {
        form { class: "orgs-form", onsubmit: submit,
            div { class: "orgs-field",
                Label { html_for: "new-name", "Name" }
                Input {
                    id: "new-name",
                    placeholder: "{name_placeholder}",
                    value: name(),
                    oninput: move |evt: FormEvent| {
                        name.set(evt.value());
                        if !slug_edited() {
                            slug.set(slugify(&evt.value()));
                        }
                    },
                }
            }
            div { class: "orgs-field",
                Label { html_for: "new-slug", "Slug" }
                Input {
                    id: "new-slug",
                    placeholder: "{slug_placeholder}",
                    value: slug(),
                    aria_invalid: blocked,
                    oninput: move |evt: FormEvent| {
                        slug_edited.set(!evt.value().is_empty());
                        slug.set(evt.value());
                    },
                }
            }
            Button { r#type: "submit", disabled: busy() || blocked, "Create" }
        }
        if let Some(c) = verdict {
            p { class: if c.available { "slug-check ok" } else { "slug-check err" }, "{c.message}" }
        }
        if let Some(message) = error() {
            p { class: "orgs-error", "{message}" }
        }
    }
}
