//! Shared fullstack server functions — the `core` service of
//! `docs/architecture.md` §2, in its minimal form: orgs, backed by the core
//! schema (`migrations/0001_core.sql`).
//!
//! The schema is applied by `typednotes-infra`, never by this server, so the
//! server's database identity has data rights only. There is no
//! authentication yet — the IdP (`docs/services/idp.md`) is not built — so
//! every endpoint here is public. Do not store anything real in a deployment
//! of this version.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
mod db;

/// An org, as the UI sees it. Ids and timestamps travel as text: the client
/// only displays them, and this keeps `uuid`/`chrono` out of the wasm build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Org {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub created_at: String,
}

/// Whether the server can reach its database and sees the core schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub database: bool,
    pub schema: bool,
}

/// Why an org name or slug was refused, checked identically on both sides so
/// the form can explain before a round trip and the server still enforces it.
pub fn validate_org(slug: &str, name: &str) -> Result<(), String> {
    let slug_ok = (3..=40).contains(&slug.len())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-');
    if !slug_ok {
        return Err(
            "slug: 3–40 characters, lowercase letters, digits and inner dashes".to_string(),
        );
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err("name: 1–100 characters".to_string());
    }
    Ok(())
}

/// Database reachability and schema presence, for the status line and for
/// probing a fresh deploy.
#[get("/api/health")]
pub async fn health() -> Result<Health, ServerFnError> {
    Ok(db::health().await)
}

/// Every org, newest first.
#[get("/api/orgs")]
pub async fn list_orgs() -> Result<Vec<Org>, ServerFnError> {
    db::list_orgs().await
}

/// Create an org. `409` if the slug is taken, `400` if it is malformed.
#[post("/api/orgs")]
pub async fn create_org(slug: String, name: String) -> Result<Org, ServerFnError> {
    let slug = slug.trim().to_lowercase();
    validate_org(&slug, &name)
        .map_err(|message| ServerFnError::ServerError { message, code: 400, details: None })?;
    db::create_org(&slug, name.trim()).await
}

#[cfg(test)]
mod tests {
    use super::validate_org;

    #[test]
    fn accepts_a_plain_slug() {
        assert!(validate_org("acme-labs", "Acme Labs").is_ok());
    }

    #[test]
    fn refuses_malformed_slugs() {
        for slug in ["ab", "Acme", "acme_labs", "-acme", "acme-", "acme labs"] {
            assert!(validate_org(slug, "Acme").is_err(), "{slug} should be refused");
        }
    }

    #[test]
    fn refuses_blank_or_long_names() {
        assert!(validate_org("acme", "   ").is_err());
        assert!(validate_org("acme", &"x".repeat(101)).is_err());
    }
}
