use api::list_ai_models;
use dioxus::prelude::*;
use crate::components::{button::{Button, ButtonSize, ButtonVariant}, select::{Select, SelectOption}};
use crate::error_message;

/// Provider inventory is loaded through the existing models.list broker grant.
/// Key this component by connection ID so late results cannot cross accounts.
#[component]
pub(crate) fn ModelPicker(slug: String, connection_id: String, mut value: Signal<String>, #[props(default = false)] disabled: bool) -> Element {
    let mut models = use_resource(move || {
        let (slug, id) = (slug.clone(), connection_id.clone());
        async move { list_ai_models(slug, id).await.map_err(|e| error_message(&e)) }
    });
    let selected = use_memo(move || {
        models().and_then(Result::ok).filter(|ids| ids.contains(&value())).map(|_| value())
    });
    rsx! {
        div { class: "conn-subform model-picker",
            match models() {
                None => rsx! { p { class: "conn-meta", role: "status", "Loading provider models…" } },
                Some(Err(message)) => rsx! { p { class: "orgs-error", role: "alert", "Could not load models: {message}" } },
                Some(Ok(ids)) if ids.is_empty() => rsx! { p { class: "conn-meta", "This provider returned no models for this account." } },
                Some(Ok(ids)) => rsx! {
                    div { class: "conn-select",
                        Select::<String> {
                            value: Some(selected.into()),
                            aria_label: "Provider model", disabled,
                            on_value_change: move |id: Option<String>| { if let Some(id) = id { value.set(id); } },
                            for (index, id) in ids.iter().enumerate() {
                                SelectOption::<String> { key: "{id}", index, value: id.clone(), text_value: id.clone(), "{id}" }
                            }
                        }
                    }
                    if !value().is_empty() && !ids.contains(&value()) {
                        p { class: "conn-meta", "Saved model “{value}” is absent from this provider catalog. Choose an available model to change it." }
                    }
                },
            }
            Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Ghost,
                disabled: disabled || models.state()() == UseResourceState::Pending,
                onclick: move |_| models.restart(), "Refresh models" }
        }
    }
}
