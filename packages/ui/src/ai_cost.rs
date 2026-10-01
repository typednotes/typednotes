use crate::components::input::Input;
use crate::components::label::Label;
use api::{ai_token_pricing, Provider, TokenPricing};
use dioxus::prelude::*;

fn rate(value: Option<f64>) -> String {
    value
        .map(|n| format!("${n:.6}"))
        .unwrap_or_else(|| "Unavailable".into())
}

/// Public catalog estimates only. Quotas, subscriptions and ledger credits are
/// not translated into a pretend dollar bill.
#[component]
pub(crate) fn ModelCost(provider: ReadSignal<Provider>, model: ReadSignal<String>) -> Element {
    let pricing = use_resource(move || {
        let (p, m) = (provider(), model());
        async move {
            if m.trim().is_empty() {
                Ok(TokenPricing {
                    provider: p,
                    model: m,
                    input: None,
                    output: None,
                    cache_read: None,
                    cache_write: None,
                    source: None,
                    subscription: false,
                    note: "Enter a model ID to see token pricing.".into(),
                })
            } else {
                futures_timer::Delay::new(std::time::Duration::from_millis(400)).await;
                ai_token_pricing(p, m).await
            }
        }
    });
    let mut input_tokens = use_signal(|| 1000_u64);
    let mut output_tokens = use_signal(|| 1000_u64);
    rsx! {
        div { class: "conn-subform", aria_live: "polite",
            h4 { "Token cost" }
            match pricing() {
                None => rsx! { p { class: "conn-meta", "Loading published rates…" } },
                Some(Err(_)) => rsx! { p { class: "conn-meta", "Pricing unavailable for this model." } },
                Some(Ok(p)) => rsx! {
                    p { class: "conn-meta", "USD per 1 million tokens · model: {p.model}" }
                    if !p.subscription {
                        p { class: "conn-meta", "Input: {rate(p.input)} · Output: {rate(p.output)}" }
                    }
                    if !p.subscription && (p.cache_read.is_some() || p.cache_write.is_some()) {
                        p { class: "conn-meta", "Cache read: {rate(p.cache_read)} · Cache write: {rate(p.cache_write)}" }
                    }
                    if p.subscription {
                        p { class: "conn-meta", "Subscription/token plan: billing is quota/plan based. A zero catalog rate does not mean unlimited free usage; monthly charges and request multipliers are not represented here." }
                    } else if p.input.is_some() && p.output.is_some() {
                        div { class: "conn-grid",
                            div { class: "orgs-field",
                                    Label { html_for: "cost-input-tokens", "Planned input tokens" }
                                Input { id: "cost-input-tokens", r#type: "number", min: "0", value: input_tokens().to_string(),
                                    oninput: move |e: FormEvent| { if let Ok(n) = e.value().parse() { input_tokens.set(n); } } }
                            }
                            div { class: "orgs-field",
                                Label { html_for: "cost-output-tokens", "Planned output tokens" }
                                Input { id: "cost-output-tokens", r#type: "number", min: "0", value: output_tokens().to_string(),
                                    oninput: move |e: FormEvent| { if let Ok(n) = e.value().parse() { output_tokens.set(n); } } }
                            }
                        }
                        if let Some(estimate) = p.estimate(input_tokens(),output_tokens(),0,0) {
                            p { class: "conn-meta", "Estimated request cost: ${estimate:.6} (uncached tokens)" }
                            p { class: "conn-meta", "This provider-cost estimate is separate from Typednotes credits." }
                        }
                    }
                    if let Some(source) = p.source.clone() {
                        p { class: "conn-meta", a { href: source, target: "_blank", rel: "noopener noreferrer", "Pricing source" } " · {p.note}" }
                    } else {
                        p { class: "conn-meta", "Pricing unavailable: no published rate was found. Unknown prices are not treated as free." }
                    }
                },
            }
        }
    }
}
