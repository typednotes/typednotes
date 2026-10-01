//! Per-user compute (docs/computations.md §4.1): a schema and a role, both
//! named after the org and the user, on `compute-db` — the database that
//! holds `db` sinks' data and nothing else.
//!
//! Provisioned lazily, the first time a session of a graph with a `db` sink
//! is registered for that (org, user). The role owns its schema and holds no
//! other grant. Its password is generated here, goes to the vault at
//! `secret/data/compute/{org_id}/{user_id}` (where `lun` reads it as its own
//! identity; the app can only create and delete there), and crosses
//! Postgres only as a SCRAM-SHA-256 verifier — never in the clear.
//!
//! The DDL runs in one transaction on `compute-db`, committed only once the
//! vault has the credential: a failure leaves neither a role nobody can log
//! in as nor a credential for a role that does not exist.

use std::sync::OnceLock;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{AssertSqlSafe, Row};

use super::{config, vault};

/// The schema and role name for (org, user): `{org_slug}_{user_id}`, dashes
/// to underscores. Postgres truncates identifiers at 63 bytes, so a longer
/// name keeps the slug's first 21 characters and adds the org id's first 8
/// hex digits — still readable, still distinct per org (and `compute_schemas`
/// is unique on the name).
pub fn role_name(org_slug: &str, org_id: &str, user_id: &str) -> String {
    let slug = org_slug.to_ascii_lowercase().replace('-', "_");
    let user = user_id.to_ascii_lowercase().replace('-', "_");
    let full = format!("{slug}_{user}");
    if full.len() <= 63 {
        return full;
    }
    let short: String = slug.chars().take(21).collect();
    let org: String = org_id.replace('-', "").chars().take(8).collect();
    let user: String = user_id.replace('-', "");
    format!("{}_{org}_{user}", short.trim_end_matches('_'))
}

/// Only names [`role_name`] makes are ever spliced into DDL.
fn is_safe_ident(s: &str) -> bool {
    (1..=63).contains(&s.len())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// PBKDF2-HMAC-SHA256 with a 32-byte output: one block (RFC 8018 §5.2).
fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32) -> Vec<u8> {
    let mut block = salt.to_vec();
    block.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac(password, &block);
    let mut out = u.clone();
    for _ in 1..iterations {
        u = hmac(password, &u);
        for (o, x) in out.iter_mut().zip(&u) {
            *o ^= x;
        }
    }
    out
}

/// The verifier Postgres stores for a SCRAM-SHA-256 password (RFC 5802,
/// RFC 7677, as `pg_authid.rolpassword` holds it):
/// `SCRAM-SHA-256$<iterations>:<salt>$<StoredKey>:<ServerKey>`. The password
/// must be ASCII without spaces, so SASLprep leaves it unchanged.
pub fn scram_verifier(password: &str, salt: &[u8], iterations: u32) -> String {
    let salted = pbkdf2_sha256(password.as_bytes(), salt, iterations);
    let client_key = hmac(&salted, b"Client Key");
    let stored_key = Sha256::digest(&client_key);
    let server_key = hmac(&salted, b"Server Key");
    format!(
        "SCRAM-SHA-256${iterations}:{}${}:{}",
        STANDARD.encode(salt),
        STANDARD.encode(stored_key),
        STANDARD.encode(server_key)
    )
}

/// Only verifiers [`scram_verifier`] makes are spliced into DDL.
fn is_safe_verifier(v: &str) -> bool {
    v.starts_with("SCRAM-SHA-256$")
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || "$:+/=-".contains(c))
}

static POOL: OnceLock<Result<PgPool, String>> = OnceLock::new();

fn pool() -> Result<&'static PgPool, String> {
    POOL.get_or_init(|| {
        let url =
            config::compute_db_url().ok_or("db sinks are disabled: COMPUTE_DB_URL is not set")?;
        PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(30))
            .connect_lazy(&url)
            .map_err(|e| format!("COMPUTE_DB_URL is not a valid connection string: {e}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// `host:port` and database of `COMPUTE_DB_URL`, for the credential lun
/// connects with. Never the URL's own credentials.
fn target() -> Result<(String, String), String> {
    let raw = config::compute_db_url().ok_or("COMPUTE_DB_URL is not set")?;
    target_from_urls(&raw, config::env("COMPUTE_DB_RUNTIME_URL").as_deref())
}

fn target_from_urls(raw: &str, runtime: Option<&str>) -> Result<(String, String), String> {
    let local = url::Url::parse(raw).map_err(|_| "COMPUTE_DB_URL is not a URL")?;
    let url = match runtime {
        Some(raw) => {
            let remote = url::Url::parse(raw).map_err(|_| "COMPUTE_DB_RUNTIME_URL is not a URL")?;
            if !matches!(remote.scheme(), "postgres" | "postgresql") || !remote.username().is_empty() || remote.password().is_some()
                || remote.query().is_some() || remote.fragment().is_some() || remote.path() != local.path() {
                return Err("COMPUTE_DB_RUNTIME_URL must name the same database, without credentials, query or fragment".into());
            }
            remote
        }
        None => local,
    };
    let host = url.host_str().ok_or("compute database has no host")?;
    let port = url.port().unwrap_or(5432);
    let database = url.path().trim_start_matches('/');
    let database = if database.is_empty() {
        "compute"
    } else {
        database
    };
    Ok((format!("{host}:{port}"), database.to_string()))
}

/// Non-secret compilation target. The API key/password and administrative URL
/// credentials are never part of this value or a writer/model request.
pub fn public_target(schema: &str) -> Result<serde_json::Value, String> {
    if !is_safe_ident(schema) { return Err("invalid bound compute schema".into()); }
    let (base, database) = target()?;
    let (host, port) = base.rsplit_once(':').ok_or("invalid compute host/port")?;
    let port: u16 = port.parse().map_err(|_| "invalid compute port")?;
    if port == 0 || !host.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '.') {
        return Err("the runtime requires a plain compute hostname and port".into());
    }
    Ok(json!({"host": host, "port": port, "database": database, "user": schema, "schema": schema}))
}

/// The vault path (under `secret/data/`) of (org, user)'s compute credential.
pub fn credential_path(org_id: &str, user_id: &str) -> String {
    format!("compute/{org_id}/{user_id}")
}

/// Make sure (org, user) has its schema and role; their name.
pub async fn ensure(org_id: &str, org_slug: &str, user_id: &str) -> Result<String, String> {
    let mut app = super::connector::lock(org_id).await.map_err(|e| super::errors::message(&e))?;
    if let Some(row) = sqlx::query(
        "select name from compute_schemas where org_id = $1::uuid and user_id = $2::uuid",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(&mut *app)
    .await
    .map_err(|e| format!("database error: {e}"))?
    {
        let name: String = row.get("name");
        if name != role_name(org_slug, org_id, user_id) || !is_safe_ident(&name) {
            return Err("stored compute schema does not match its organization/user binding".into());
        }
        return Ok(name);
    }
    let name = role_name(org_slug, org_id, user_id);
    if !is_safe_ident(&name) {
        return Err(format!("cannot name a compute schema '{name}'"));
    }
    let (host, database) = target()?;
    let password = super::session::random_token(32).map_err(|e| super::errors::message(&e))?;
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| format!("no randomness: {e}"))?;
    let verifier = scram_verifier(&password, &salt, 4096);
    if !is_safe_verifier(&verifier) {
        return Err("unexpected verifier".to_string());
    }

    let mut tx = pool()?
        .begin()
        .await
        .map_err(|e| format!("the notebook database is unreachable: {e}"))?;
    let statements = [
        format!(
            "create role \"{name}\" login noinherit nocreatedb nocreaterole password '{verifier}'"
        ),
        format!("alter role \"{name}\" set search_path = \"{name}\""),
        // To create a schema owned by the role, its creator must be able to
        // act as it for the length of this transaction.
        format!("grant \"{name}\" to current_user"),
        format!("create schema \"{name}\" authorization \"{name}\""),
        format!("revoke \"{name}\" from current_user"),
        format!("revoke all on schema public from \"{name}\""),
    ];
    for statement in statements {
        sqlx::query(AssertSqlSafe(statement))
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("the notebook database refused to provision {name}: {e}"))?;
    }
    let credential = json!({
        "kind": "postgres", "base_url": host, "database": database,
        "schema": name, "token": password,
    });
    vault::write(&credential_path(org_id, user_id), &credential)
        .await
        .map_err(|e| format!("could not store the compute credential: {e}"))?;
    if let Err(e) = tx.commit().await {
        let _ = vault::delete(&credential_path(org_id, user_id)).await;
        return Err(format!("the notebook database did not commit {name}: {e}"));
    }
    sqlx::query(
        "insert into compute_schemas (org_id, user_id, name) values ($1::uuid, $2::uuid, $3) \
         on conflict do nothing",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(&name)
    .execute(&mut *app)
    .await
    .map_err(|e| format!("database error: {e}"))?;
    app.commit().await.map_err(|_| "could not commit the compute binding")?;
    Ok(name)
}

/// The conventional database sink stores one JSON value column. Provisioning
/// is trusted app DDL, separate from the runtime's operation-scoped query API.
pub async fn ensure_sink_table(org_id: &str, org_slug: &str, user_id: &str, table: &str) -> Result<String, String> {
    if !is_safe_ident(table) { return Err("invalid database sink table identifier".into()); }
    let schema = ensure(org_id, org_slug, user_id).await?;
    let _guard = super::connector::lock(org_id).await.map_err(|e| super::errors::message(&e))?;
    let mut tx = pool()?.begin().await.map_err(|_| "compute database unavailable")?;
    for statement in [format!("grant \"{schema}\" to current_user"), format!("set local role \"{schema}\""),
        format!("create table if not exists \"{schema}\".\"{table}\" (value jsonb not null)"),
        "reset role".into(), format!("revoke \"{schema}\" from current_user")] {
        sqlx::query(AssertSqlSafe(statement)).execute(&mut *tx).await.map_err(|_| "could not provision the bound database sink table")?;
    }
    tx.commit().await.map_err(|_| "could not commit database sink provisioning")?;
    Ok(schema)
}

/// Drop a compute schema, the data in it, and its role — when the org is
/// deleted. The app's identity acts as the role for the length of the
/// transaction, which dropping what the role owns requires.
pub async fn drop_schema(name: &str) -> Result<(), String> {
    if !is_safe_ident(name) {
        return Err(format!("not a compute schema name: {name}"));
    }
    let mut tx = pool()?
        .begin()
        .await
        .map_err(|e| format!("the notebook database is unreachable: {e}"))?;
    for statement in [
        format!("grant \"{name}\" to current_user"),
        format!("drop schema if exists \"{name}\" cascade"),
        format!("revoke \"{name}\" from current_user"),
        format!("drop role if exists \"{name}\""),
    ] {
        sqlx::query(AssertSqlSafe(statement))
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("the notebook database refused to drop {name}: {e}"))?;
    }
    tx.commit()
        .await
        .map_err(|e| format!("the notebook database did not commit dropping {name}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_compute_address_changes_transport_only() {
        let local = "postgres://admin:private@127.0.0.1:15433/compute";
        assert_eq!(target_from_urls(local, Some("postgres://compute-db:5432/compute")).unwrap(), ("compute-db:5432".into(), "compute".into()));
        for remote in ["postgres://compute-db:5432/other", "postgres://admin:private@compute-db/compute", "https://compute-db/compute", "postgres://compute-db/compute?schema=other"] {
            assert!(target_from_urls(local, Some(remote)).is_err());
        }
    }

    #[test]
    fn names() {
        let org = "0b8c2f4e-1111-2222-3333-444455556666";
        let user = "a1b2c3d4-aaaa-bbbb-cccc-ddddeeeeffff";
        assert_eq!(
            role_name("acme", org, user),
            "acme_a1b2c3d4_aaaa_bbbb_cccc_ddddeeeeffff"
        );
        let long = role_name("a-very-long-organisation-slug-indeed", org, user);
        assert!(long.len() <= 63, "{long}");
        assert_eq!(
            long,
            "a_very_long_organisat_0b8c2f4e_a1b2c3d4aaaabbbbccccddddeeeeffff"
        );
        assert!(is_safe_ident(&long));
        assert!(!is_safe_ident("x\"; drop"));
    }

    /// RFC 7677 §3's test vector: user `user`, password `pencil`, salt
    /// `W22ZaJ0SNY7soEsUEjb6gQ==`, 4096 iterations.
    #[test]
    fn scram_matches_rfc_7677() {
        let salt = STANDARD.decode("W22ZaJ0SNY7soEsUEjb6gQ==").unwrap();
        let v = scram_verifier("pencil", &salt, 4096);
        let salted = pbkdf2_sha256(b"pencil", &salt, 4096);
        // ServerSignature for the RFC's AuthMessage is derived from ServerKey;
        // the RFC gives the final signature `6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=`.
        let auth_message = "n=user,r=rOprNGfwEbeRWgbNEkqO,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096,c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";
        let server_key = hmac(&salted, b"Server Key");
        let signature = hmac(&server_key, auth_message.as_bytes());
        assert_eq!(
            STANDARD.encode(signature),
            "6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4="
        );
        assert!(v.starts_with("SCRAM-SHA-256$4096:W22ZaJ0SNY7soEsUEjb6gQ==$"));
        assert!(is_safe_verifier(&v));
        assert!(!is_safe_verifier("SCRAM-SHA-256$x' or '1"));
    }

    #[test]
    fn credential_path_is_org_then_user() {
        assert_eq!(credential_path("o", "u"), "compute/o/u");
    }
}
