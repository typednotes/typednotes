//! Trusted native minting. The database and write-only vault are a distributed
//! boundary: publish ceilings before returning a warrant; deny before changing
//! policy. Every publisher holds the same per-org SQL transaction lock.
use dioxus::prelude::ServerFnError;
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};

use super::{db, errors, vault, warrant};
use crate::{Connection, ConnectorPermissions, EffectPolicy, Org, PermissionPreset};

pub fn organization_path(org: &str, provider: &str, connection: &str) -> String {
    format!("connector-policy/{org}/{provider}/{connection}")
}

pub fn authority_path(org: &str, run: &str, warrant: &str) -> String {
    format!("connector-authority/{org}/{run}/{warrant}")
}

/// Serializes policy publication across processes, including scheduled calls.
pub async fn lock(org: &str) -> Result<Transaction<'static, Postgres>, ServerFnError> {
    let mut tx = db::pool()?.begin().await.map_err(errors::db_error)?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(org).execute(&mut *tx).await.map_err(errors::db_error)?;
    Ok(tx)
}

pub async fn policy(tx: &mut Transaction<'_, Postgres>, org: &str) -> Result<EffectPolicy, ServerFnError> {
    let row = sqlx::query("select effect_policy::text as policy from orgs where id = $1::uuid")
        .bind(org).fetch_one(&mut **tx).await.map_err(errors::db_error)?;
    let policy = row.get::<Option<String>, _>("policy").map(|s| serde_json::from_str::<EffectPolicy>(&s))
        .transpose().map_err(|_| errors::forbidden("invalid organization connector policy"))?.unwrap_or_default();
    policy.validate().map_err(errors::forbidden)
}

pub fn ceiling(policy: &EffectPolicy, connection: &Connection, parent: &ConnectorPermissions) -> ConnectorPermissions {
    if !policy.allows_connector(connection.provider) {
        return ConnectorPermissions::deny_all();
    }
    policy.connector_ceilings.get(connection.provider.id()).cloned().unwrap_or_else(|| parent.clone())
}

pub async fn publish_ceiling(org: &str, provider: &str, connection: &str, ceiling: &ConnectorPermissions) -> Result<(), ServerFnError> {
    vault::write(&organization_path(org, provider, connection), &json!(ceiling))
        .await.map_err(|_| errors::bad_gateway("could not provision the trusted organization connector policy"))
}

/// Revoke all known projections before a connection/org edit is acknowledged.
/// Deleting an authority is deny-all at the broker, never a default grant.
pub async fn revoke(tx: &mut Transaction<'_, Postgres>, org: &str, connection: Option<&str>) -> Result<(), ServerFnError> {
    let rows = sqlx::query("select warrant_id::text as warrant, run_id::text as run from connector_authorities \
        where org_id = $1::uuid and ($2::text is null or connection_id::text = $2)")
        .bind(org).bind(connection).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for row in rows {
        vault::delete(&authority_path(org, &row.get::<String, _>("run"), &row.get::<String, _>("warrant")))
            .await.map_err(|_| errors::bad_gateway("could not revoke the trusted connector authority"))?;
    }
    sqlx::query("delete from connector_authorities where org_id = $1::uuid and ($2::text is null or connection_id::text = $2)")
        .bind(org).bind(connection).execute(&mut **tx).await.map_err(errors::db_error)?;
    Ok(())
}

pub async fn revoke_cell(tx: &mut Transaction<'_, Postgres>, org: &str, cell: &str) -> Result<(), ServerFnError> {
    let graph = sqlx::query("select g.id::text as graph from graph_cells c join graphs g on g.id=c.graph_id \
        join projects p on p.id=g.project_id where c.id=$1::uuid and p.org_id=$2::uuid")
        .bind(cell).bind(org).fetch_optional(&mut **tx).await.map_err(errors::db_error)?;
    if let Some(graph) = graph {
        revoke_writer(tx, org, &graph.get::<String, _>("graph")).await?;
    }
    let rows = sqlx::query("select warrant_id::text as warrant, run_id::text as run from connector_authorities where org_id = $1::uuid and cell_id = $2::uuid")
        .bind(org).bind(cell).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for row in rows {
        vault::delete(&authority_path(org, &row.get::<String, _>("run"), &row.get::<String, _>("warrant")))
            .await.map_err(|_| errors::bad_gateway("could not revoke cell connector authority"))?;
    }
    sqlx::query("delete from connector_authorities where org_id = $1::uuid and cell_id = $2::uuid")
        .bind(org).bind(cell).execute(&mut **tx).await.map_err(errors::db_error)?;
    Ok(())
}

pub async fn revoke_graph(tx: &mut Transaction<'_, Postgres>, org: &str, graph: &str) -> Result<(), ServerFnError> {
    revoke_writer(tx, org, graph).await?;
    let rows = sqlx::query("select warrant_id::text as warrant, run_id::text as run from connector_authorities where org_id = $1::uuid and graph_id = $2::uuid")
        .bind(org).bind(graph).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for row in rows {
        vault::delete(&authority_path(org, &row.get::<String, _>("run"), &row.get::<String, _>("warrant")))
            .await.map_err(|_| errors::bad_gateway("could not revoke graph authority"))?;
    }
    sqlx::query("delete from connector_authorities where org_id = $1::uuid and graph_id = $2::uuid")
        .bind(org).bind(graph).execute(&mut **tx).await.map_err(errors::db_error)?;
    Ok(())
}

/// Cell/graph declaration edits also revoke anonymous HTTP/files authority,
/// which has no warrant-keyed broker row. The same org transaction serializes
/// edits with minting; Lode serializes this narrowing with actual tool effects.
async fn revoke_writer(tx: &mut Transaction<'_, Postgres>, org: &str, graph: &str) -> Result<(), ServerFnError> {
    let row = sqlx::query("select g.lode_session_id as session from graphs g join projects p on p.id=g.project_id \
        where g.id=$1::uuid and p.org_id=$2::uuid and g.lode_session_id is not null")
        .bind(graph).bind(org).fetch_optional(&mut **tx).await.map_err(errors::db_error)?;
    if let Some(row) = row {
        let tools = policy(tx, org).await?.tools;
        super::lode::narrow(&row.get::<String, _>("session"), &tools).await
            .map_err(|_| errors::bad_gateway("could not revoke the writer's cell/graph effect authority"))?;
    }
    Ok(())
}

pub struct Minted {
    pub grant: warrant::Grant,
    pub organization: ConnectorPermissions,
    pub connection: ConnectorPermissions,
    pub cell: ConnectorPermissions,
}

/// Server-derived selectors inherit byte ceilings and intersect resource grants;
/// this is attenuation, not a user-requested policy edit.
pub fn scoped(connection: &Connection, operation: &str, root: Vec<String>, descendants: bool) -> ConnectorPermissions {
    let parent = connection.permissions.clone().unwrap_or_else(|| ConnectorPermissions::preset(connection.provider, PermissionPreset::ReadOnly));
    parent.intersect(&ConnectorPermissions::scoped(operation, root, descendants))
}

/// ObjectStore's logical bucket is non-secret metadata, not a credential or a
/// caller-selected URL. Legacy path-style bases have one bucket/container path.
pub fn bucket(connection: &Connection) -> Option<String> {
    if !matches!(connection.provider, crate::Provider::S3 | crate::Provider::Azure) { return None; }
    let bucket = connection.external_id.clone().or_else(|| {
        let url = url::Url::parse(&connection.base_url).ok()?;
        let parts: Vec<_> = url.path_segments()?.filter(|part| !part.is_empty()).collect();
        match parts.as_slice() {
            [bucket] => Some((*bucket).to_string()),
            [] if connection.provider == crate::Provider::S3 && url.host_str()?.contains(".s3.") =>
                Some(url.host_str()?.split('.').next()?.to_string()),
            _ => None,
        }
    })?;
    ConnectorPermissions::valid_resource(&[bucket.clone()]).then_some(bucket)
}

/// The request can attenuate, never replace, a fresh connection ceiling.
/// Stale DTOs cannot re-publish authority after an admin's policy change.
pub async fn mint(org: &Org, connection: &Connection, owner: &str, operation: &str,
    requested: &ConnectorPermissions, cost: u64) -> Result<Minted, ServerFnError> {
    mint_for_cell(org, connection, owner, operation, requested, cost, None).await
}

/// A cell mint reads its declaration under the same lock used by declaration
/// edits. A stale request cannot restore a removed/wider cell permission.
pub async fn mint_for_cell(org: &Org, connection: &Connection, owner: &str, operation: &str,
    requested: &ConnectorPermissions, cost: u64, cell_id: Option<&str>) -> Result<Minted, ServerFnError> {
    requested.validate(connection.provider).map_err(errors::bad_request)?;
    let root = warrant::root_key().map_err(errors::unavailable)?;
    let mut tx = lock(&org.id).await?;
    let row = sqlx::query("select permissions::text as permissions, user_id::text as owner, provider, status \
        from connections where org_id = $1::uuid and id = $2::uuid")
        .bind(&org.id).bind(&connection.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?;
    if row.get::<String, _>("owner") != owner || row.get::<String, _>("provider") != connection.provider.id()
        || !["active", "failed"].contains(&row.get::<String, _>("status").as_str()) {
        return Err(errors::forbidden("connection identity or provisioning state changed"));
    }
    let parent = row.get::<Option<String>, _>("permissions").map(|s| serde_json::from_str::<ConnectorPermissions>(&s))
        .transpose().map_err(|_| errors::forbidden("invalid connection permissions"))?
        .unwrap_or_else(|| ConnectorPermissions::preset(connection.provider, PermissionPreset::ReadOnly));
    parent.validate(connection.provider).map_err(errors::forbidden)?;
    let mut graph_id = None;
    if let Some(cell_id) = cell_id {
        let row = sqlx::query("select c.config::text as config, g.id::text as graph from graph_cells c join graphs g on g.id = c.graph_id \
            join projects p on p.id = g.project_id where c.id = $1::uuid and p.org_id = $2::uuid")
            .bind(cell_id).bind(&org.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?
            .ok_or_else(|| errors::forbidden("the connector cell is gone or belongs to another organization"))?;
        graph_id = Some(row.get::<String, _>("graph"));
        let config: crate::CellConfig = serde_json::from_str(&row.get::<String, _>("config"))
            .map_err(|_| errors::forbidden("invalid stored cell connector declaration"))?;
        let selected = config.connectors.iter().find(|grant| grant.connection == connection.id);
        let mut declared = match selected {
            Some(grant) => grant.permissions.clone().unwrap_or_else(|| parent.clone()),
            None if config.connection_id.as_deref() == Some(connection.id.as_str()) => {
                let expected = match connection.provider {
                    crate::Provider::S3 | crate::Provider::Azure => "objects.write",
                    crate::Provider::Dropbox => "files.create",
                    _ => return Err(errors::forbidden("unsupported native storage declaration")),
                };
                scoped(connection, expected, config.path.as_deref().ok_or_else(|| errors::forbidden("storage cell has no path"))?
                    .trim_start_matches('/').split('/').map(str::to_string).collect(), false)
            },
            None if config.channel_id.is_some() => {
                let channel = sqlx::query("select ch.external_id, ch.external_channel from channels ch join projects p on p.id = ch.project_id \
                    where ch.id = $1::uuid and ch.connection_id = $2::uuid and p.org_id = $3::uuid")
                    .bind(&config.channel_id).bind(&connection.id).bind(&org.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?
                    .ok_or_else(|| errors::forbidden("cell interface connection changed"))?;
                let resource = if connection.provider == crate::Provider::Slack {
                    vec![channel.get::<Option<String>, _>("external_channel").ok_or_else(|| errors::forbidden("interface has no channel"))?]
                } else {
                    let peer = crate::validate_phone(config.recipient.as_deref().unwrap_or("")).map_err(errors::forbidden)?;
                    vec![channel.get("external_id"), if connection.provider == crate::Provider::Signal { format!("+{peer}") } else { peer }]
                };
                scoped(connection, "messages.send", resource, false)
            },
            None => return Err(errors::forbidden("the cell no longer selects this connection")),
        };
        // Legacy sink selectors remain upper bounds even when that cell also
        // selects an advanced grant for the same connection.
        if selected.is_some() && config.connection_id.as_deref() == Some(connection.id.as_str()) {
            let expected = match connection.provider {
                crate::Provider::S3 | crate::Provider::Azure => "objects.write",
                crate::Provider::Dropbox => "files.create",
                _ => return Err(errors::forbidden("unsupported native storage declaration")),
            };
            let path = config.path.as_deref().ok_or_else(|| errors::forbidden("storage cell has no path"))?;
            declared = declared.intersect(&scoped(connection, expected, path.trim_start_matches('/').split('/').map(str::to_string).collect(), false));
        }
        if selected.is_some() && config.channel_id.is_some()
            && matches!(connection.provider, crate::Provider::Slack | crate::Provider::Signal | crate::Provider::Whatsapp) {
            let channel = sqlx::query("select ch.external_id, ch.external_channel from channels ch join projects p on p.id = ch.project_id \
                where ch.id = $1::uuid and ch.connection_id = $2::uuid and p.org_id = $3::uuid")
                .bind(&config.channel_id).bind(&connection.id).bind(&org.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?
                .ok_or_else(|| errors::forbidden("cell interface connection changed"))?;
            let resource = if connection.provider == crate::Provider::Slack {
                vec![channel.get::<Option<String>, _>("external_channel").ok_or_else(|| errors::forbidden("interface has no channel"))?]
            } else {
                let peer = crate::validate_phone(config.recipient.as_deref().unwrap_or("")).map_err(errors::forbidden)?;
                vec![channel.get("external_id"), if connection.provider == crate::Provider::Signal { format!("+{peer}") } else { peer }]
            };
            declared = declared.intersect(&scoped(connection, "messages.send", resource, false));
        }
        declared = parent.narrow(&declared.validate(connection.provider).map_err(errors::forbidden)?).map_err(errors::forbidden)?;
        declared.narrow(requested).map_err(errors::forbidden)?;
    }
    let policy = policy(&mut tx, &org.id).await?;
    let organization = ceiling(&policy, connection, &parent);
    let cell = parent.narrow(requested).map_err(errors::forbidden)?.intersect(&organization);
    if !cell.scopes.iter().any(|s| s.operation == operation) {
        return Err(errors::forbidden(policy.connector_blocker(connection.provider, &parent, operation, &[])
            .unwrap_or_else(|| format!("{operation} is not permitted by the live operation/resource ceilings"))));
    }
    let grant = warrant::for_connection(&root, &org.id, connection.provider.id(), operation, &connection.id, cost);
    let account = format!("{owner}/{}", connection.id);
    let capability = cell.capability_json(connection.provider, &connection.id);
    let projection = json!({"account": account, "cell": capability, "warrant": capability});
    // Connection defaults are explicitly materialized; malformed/missing trusted
    // organization/run documents never trigger the broker's legacy read default.
    vault::write(&format!("{}/permissions", vault::credential_path(connection.provider, owner, &connection.id)), &json!(parent))
        .await.map_err(|_| errors::bad_gateway("could not provision the connection ceiling"))?;
    publish_ceiling(&org.id, connection.provider.id(), &connection.id, &organization).await?;
    sqlx::query("insert into connector_authorities (warrant_id, org_id, connection_id, run_id, account, provider, expires_at, graph_id, cell_id, connection_ref, owner_id, projection) \
        values ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, to_timestamp($7::bigint), $8::uuid, $9::uuid, $3::text, $10::uuid, $11::jsonb)")
        .bind(&grant.warrant.id).bind(&org.id).bind(&connection.id).bind(&grant.run_id)
        .bind(&account).bind(connection.provider.id()).bind((grant.now + warrant::LIFETIME_SECS) as i64)
        .bind(graph_id).bind(cell_id)
        .bind(owner)
        .bind(projection.to_string())
        .execute(&mut *tx).await.map_err(errors::db_error)?;
    let path = authority_path(&org.id, &grant.run_id, &grant.warrant.id);
    vault::write(&path, &projection).await.map_err(|_| errors::bad_gateway("could not provision the trusted run connector authority"))?;
    if let Err(e) = tx.commit().await {
        let _ = vault::delete(&path).await;
        return Err(errors::db_error(e));
    }
    Ok(Minted { grant, organization, connection: parent, cell })
}

/// Bind only app-minted external writer warrants to a live graph and real Lode
/// session. The projection contains policy metadata, never a credential/tag.
pub struct WriterBinding {
    pub policy: EffectPolicy,
    pub functions: Vec<String>,
}

pub async fn bind_writer(org: &Org, graph: &str, session: &str, credentials: &serde_json::Value, desired_tools: &[String]) -> Result<WriterBinding, ServerFnError> {
    if session.is_empty() || session.len() > 128 || !session.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_') {
        return Err(errors::bad_request("invalid writer conversation identity"));
    }
    let mut tx = lock(&org.id).await?;
    let source = sqlx::query("select g.slug::text as slug, g.model_connection_id::text as model, p.repo_connection_id::text as repo, \
        coalesce(p.repo_default_branch,'main') as branch from graphs g join projects p on p.id = g.project_id where g.id=$1::uuid and p.org_id=$2::uuid")
        .bind(graph).bind(&org.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?;
    let cells = super::graphs::cells_of(graph).await?;
    let current = policy(&mut tx, &org.id).await?.for_cells(&cells.iter().map(|row| row.cell.clone()).collect::<Vec<_>>());
    let functions = cells.iter().filter(|row| row.cell.cell_type.has_function()).map(|row| row.cell.name.clone()).collect();
    let tools: Vec<_> = desired_tools.iter().filter(|tool| current.tools.contains(tool)).cloned().collect();
    let mut selected = Vec::new();
    for key in ["repo", "model", "lun"] {
        let credential = credentials.get(key).ok_or_else(|| errors::bad_request("missing writer credential"))?;
        selected.push((key, credential.get("warrant").ok_or_else(|| errors::bad_request("missing writer warrant"))?));
        if key == "repo" {
            if let Some(operations) = credential.get("operations").and_then(serde_json::Value::as_array) {
                for operation in operations { selected.push((key, operation.get("warrant").ok_or_else(|| errors::bad_request("missing writer operation warrant"))?)); }
            }
        }
    }
    for (kind, token) in selected {
        let id = token.get("id").and_then(serde_json::Value::as_str).ok_or_else(|| errors::bad_request("invalid writer warrant ID"))?;
        let row = sqlx::query("select run_id::text as run, connection_ref, graph_id::text as graph, projection::text as projection from connector_authorities \
            where warrant_id=$1::uuid and org_id=$2::uuid and connection_id is not null and cell_id is null and expires_at>now()")
            .bind(id).bind(&org.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?
            .ok_or_else(|| errors::forbidden("writer warrant is expired or revoked"))?;
        let expected: Option<String> = source.get(if kind == "model" { "model" } else { "repo" });
        if expected.as_deref() != Some(row.get::<String,_>("connection_ref").as_str()) {
            return Err(errors::forbidden("writer connection changed during provisioning"));
        }
        if row.get::<Option<String>,_>("graph").is_some_and(|bound| bound != graph) {
            return Err(errors::forbidden("writer warrant cannot change its graph binding"));
        }
        let mut projection: serde_json::Value = serde_json::from_str(&row.get::<String,_>("projection")).map_err(|_| errors::forbidden("invalid tracked writer projection"))?;
        if kind == "model" {
            if let Some(previous) = projection.get("conversation") {
                let names = previous.get("allowedTools").and_then(serde_json::Value::as_array).ok_or_else(|| errors::forbidden("invalid tracked conversation"))?;
                if previous.get("sessionId").and_then(serde_json::Value::as_str) != Some(session)
                    || tools.iter().any(|tool| !names.iter().any(|name| name.as_str() == Some(tool))) {
                    return Err(errors::forbidden("writer conversation cannot change identity or widen tools"));
                }
            }
            projection["conversation"] = json!({"sessionId": session, "allowedTools": tools});
        }
        if kind == "repo" {
            let publication = json!({"branch": source.get::<String,_>("branch"), "root": ["typednotes", source.get::<String,_>("slug").as_str()]});
            if projection.get("publication").is_some_and(|previous| *previous != publication) {
                return Err(errors::forbidden("writer publication boundary cannot change"));
            }
            projection["publication"] = publication;
        }
        let path = authority_path(&org.id, &row.get::<String,_>("run"), id);
        vault::write(&path, &projection).await.map_err(|_| errors::bad_gateway("could not bind trusted writer policy"))?;
        sqlx::query("update connector_authorities set projection=$2::jsonb, graph_id=$3::uuid where warrant_id=$1::uuid")
            .bind(id).bind(projection.to_string()).bind(graph).execute(&mut *tx).await.map_err(errors::db_error)?;
    }
    // Policy edits must see the new writer before generation starts. Register
    // under the very same org lock used by edits and grant publication; after
    // commit an edit either narrows this session or rejects its fresh tokens.
    sqlx::query("update graphs set lode_session_id=$2 where id=$1::uuid")
        .bind(graph).bind(session).execute(&mut *tx).await.map_err(errors::db_error)?;
    tx.commit().await.map_err(errors::db_error)?;
    Ok(WriterBinding { policy: current, functions })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trusted_paths_are_separate_and_bound_to_org_run_warrant() {
        assert_eq!(organization_path("o", "s3", "c"), "connector-policy/o/s3/c");
        assert_eq!(authority_path("o", "r", "w"), "connector-authority/o/r/w");
    }
    #[test]
    fn server_derived_scope_inherits_limits_and_does_not_widen_resources() {
        let connection = Connection {
            id: "c".into(), provider: crate::Provider::S3, label: "reports".into(),
            base_url: "https://s3.example.invalid/reports".into(), external_id: None,
            status: "active".into(), owner_email: String::new(), created_at: String::new(),
            last_checked_at: None, last_error: None, can_remove: false,
            permissions: Some(ConnectorPermissions { scopes: ConnectorPermissions::scoped("objects.write", vec!["allowed".into()], true).scopes,
                max_request_bytes: 2048, max_response_bytes: 4096 }),
        };
        let child = scoped(&connection, "objects.write", vec!["allowed".into(), "file".into()], false);
        assert_eq!(child.max_request_bytes, 2048);
        assert_eq!(child.max_response_bytes, 4096);
        assert!(child.permits("objects.write", &["allowed".into(), "file".into()]));
        assert!(!child.permits("objects.write", &["allowed".into(), "other".into()]));
        assert!(scoped(&connection, "objects.write", vec!["outside".into()], false).scopes.is_empty());
        assert_eq!(bucket(&connection).as_deref(), Some("reports"));
    }
}
