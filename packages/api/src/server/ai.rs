//! Org-scoped environment import, public price metadata, and typed classification.
use super::{config, connections, errors, oauth};
use crate::{Org, Provider, TokenPricing, User};
use dioxus::prelude::ServerFnError;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub fn environment_key(org: &Org, provider: Provider) -> Option<String> {
    if !matches!(org.role.as_str(), "owner" | "admin")
        || config::env("TYPEDNOTES_AI_ENV_ORG").as_deref() != Some(org.slug.as_str())
    {
        return None;
    }
    config::env(provider.ai_info()?.env)
}

type PriceCache = Mutex<Option<(Instant, Value)>>;
static PRICES: OnceLock<PriceCache> = OnceLock::new();

async fn catalog() -> Option<Value> {
    let mut cache = PRICES.get_or_init(|| Mutex::new(None)).lock().await;
    if let Some((at, data)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(3600) {
            return Some(data.clone());
        }
    }
    let response = oauth::http()
        .get("https://models.dev/api.json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > 16_000_000)
    {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    if bytes.len() > 16_000_000 {
        return None;
    }
    let data: Value = serde_json::from_slice(&bytes).ok()?;
    *cache = Some((Instant::now(), data.clone()));
    Some(data)
}

pub fn price_from_catalog(provider: Provider, model: &str, data: Option<&Value>) -> TokenPricing {
    let info = provider.ai_info();
    let cost = info.and_then(|p| p.pricing_id).and_then(|p| {
        data.and_then(|v| v.get(p))
            .and_then(|v| v.get("models"))
            .and_then(|v| v.get(model))
            .and_then(|v| v.get("cost"))
    });
    let rate = |k: &str| {
        cost.and_then(|c| c.get(k))
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite() && *n >= 0.0)
    };
    TokenPricing {
        provider,
        model: model.to_string(),
        input: rate("input"),
        output: rate("output"),
        cache_read: rate("cache_read"),
        cache_write: rate("cache_write"),
        source: cost.map(|_| "https://models.dev".to_string()),
        subscription: info.is_some_and(|p| p.subscription),
        note: "Catalog estimate in USD per million tokens; provider billing and quotas may differ."
            .into(),
    }
}

pub async fn pricing(provider: Provider, model: &str) -> TokenPricing {
    // Published TypeSafe rates apply to these documented model aliases only.
    if provider == Provider::TypeSafe
        && matches!(model, "jev-latest" | "jev-preview" | "jev-1.13.0")
    {
        return TokenPricing {
            provider,
            model: model.into(),
            input: Some(0.042),
            output: Some(0.0),
            cache_read: None,
            cache_write: None,
            source: Some("https://docs.typesafe.ai/models".into()),
            subscription: false,
            note: "Published Jev 1.13 rate: charged per input token; output is free.".into(),
        };
    }
    if provider == Provider::Radius {
        if let Ok(response) = oauth::http()
            .get("https://radius.pi.dev/v1/config")
            .send()
            .await
        {
            if let Ok(data) = response.json::<Value>().await {
                if let Some(m) = data.get("models").and_then(Value::as_array).and_then(|ms| {
                    ms.iter()
                        .find(|m| m.get("id").and_then(Value::as_str) == Some(model))
                }) {
                    let cost = m.get("cost");
                    let rate = |k: &str| {
                        cost.and_then(|c| c.get(k))
                            .and_then(Value::as_f64)
                            .filter(|v| v.is_finite() && *v >= 0.0)
                    };
                    return TokenPricing {
                        provider,
                        model: model.into(),
                        input: rate("input"),
                        output: rate("output"),
                        cache_read: rate("cacheRead"),
                        cache_write: rate("cacheWrite"),
                        source: Some("https://radius.pi.dev/v1/config".into()),
                        subscription: false,
                        note: "Radius published USD rates per million tokens.".into(),
                    };
                }
            }
        }
    }
    price_from_catalog(provider, model, catalog().await.as_ref())
}

pub async fn classify(
    org: &Org,
    user: &User,
    id: &str,
    request: Value,
) -> Result<Value, ServerFnError> {
    let (connection, owner) = connections::get(org, user, id).await?;
    if connection.provider != Provider::TypeSafe {
        return Err(errors::bad_request(
            "choose a TypeSafe classifier connection",
        ));
    }
    if request.get("model").and_then(Value::as_str).is_none()
        || request.get("state").is_none()
        || !request
            .get("questions")
            .and_then(Value::as_object)
            .is_some_and(|q| !q.is_empty() && q.len() <= 255)
    {
        return Err(errors::bad_request(
            "classification requires model, state and a nonempty questions object",
        ));
    }
    let model = request.get("model").and_then(Value::as_str).unwrap().to_string();
    let mut payload = request.clone();
    payload.as_object_mut().ok_or_else(|| errors::bad_request("classification payload must be an object"))?.remove("model");
    let body = payload.to_string();
    if body.len() > 1_000_000 {
        return Err(errors::bad_request("classification request is too large"));
    }
    let outcome = connections::call_with_cost(
        org,
        &connection,
        &owner,
        connections::ProviderCall::new("classification.evaluate", vec![model], payload),
        config::model_call_cost(),
    )
    .await?;
    let bytes = match outcome {
        super::liaison::Outcome::Upstream { status, body } if (200..300).contains(&status) => body,
        super::liaison::Outcome::Upstream { status, body } => {
            return Err(errors::bad_gateway(format!(
                "classifier answered {status}: {}",
                connections::snippet(&body)
            )))
        }
        super::liaison::Outcome::Refused { status, error } => {
            return Err(errors::bad_gateway(format!(
                "broker refused classification ({status} {error})"
            )))
        }
    };
    serde_json::from_slice(&bytes)
        .map_err(|_| errors::bad_gateway("the classifier returned unreadable JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn unknown_rates_are_not_free_and_estimates_use_million_token_units() {
        let data =
            json!({"openai":{"models":{"m":{"cost":{"input":2.0,"output":8.0,"cache_read":0.5}}}}});
        let p = price_from_catalog(Provider::Openai, "m", Some(&data));
        assert_eq!(p.estimate(1_000_000, 500_000, 100_000, 0), Some(6.05));
        assert!(p.estimate(0, 0, 0, 1).is_none());
        assert!(price_from_catalog(Provider::Openai, "unknown", Some(&data))
            .estimate(1, 1, 0, 0)
            .is_none());
        let plan = price_from_catalog(Provider::GithubCopilot, "m", Some(&data));
        assert!(plan.estimate(1, 1, 0, 0).is_none());
    }
}
