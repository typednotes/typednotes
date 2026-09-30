//! Operation-scoped authority for actor-bound compute and graph-vault effects.
//! Local services use the same independent policy documents as connectors, but
//! never masquerade as an external credential row or inherit a credential owner.
use dioxus::prelude::ServerFnError;
use serde_json::{json, Value};
use sqlx::Row;

use super::{compute, connector, errors, vault, warrant};
use crate::{Cell, CellConfig, CellType, ConnectorPermissions, ConnectorScope, EffectPolicy, LocalService, Org, PermissionPreset, User};

pub fn connection(service: LocalService, graph: &str) -> &str {
    match service { LocalService::Postgres => "compute", LocalService::Vault => graph }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_actor_scoped_and_sensitive_operations_remain_independent() {
        let cfg = CellConfig { table: Some("notes".into()), ..Default::default() };
        let db = declared(LocalService::Postgres, &cfg, CellType::DbSink, "actor_schema", &[]).unwrap();
        assert!(db.permits("rows.insert", &["actor_schema".into(), "notes".into()]));
        assert!(!db.permits("rows.select", &["actor_schema".into(), "notes".into()]));
        assert!(!db.permits("rows.insert", &["foreign_schema".into(), "notes".into()]));
        let vault = declared(LocalService::Vault, &CellConfig::default(), CellType::Node, "", &["token".into()]).unwrap();
        assert!(vault.permits("secrets.read", &["token".into()]));
        assert!(!vault.permits("secrets.read", &["unknown".into()]));
        assert!(!vault.permits("secrets.write", &["token".into()]));
        assert!(vault.permits("secrets.list", &[]));
        assert!(!organization(&EffectPolicy::default(), LocalService::Postgres).permits("rows.delete", &["s".into(), "t".into()]));
        let mut policy = EffectPolicy::default(); policy.effects.clear();
        assert!(organization(&policy, LocalService::Vault).scopes.is_empty());
    }
    #[test]
    fn explicit_local_scopes_cannot_leave_the_actor_schema() {
        let cfg = CellConfig { connectors: vec![crate::CellConnector { connection: "compute".into(), permissions:
            Some(ConnectorPermissions::scoped("rows.select", vec!["foreign_schema".into()], true)) }], ..Default::default() };
        assert!(declared(LocalService::Postgres, &cfg, CellType::Node, "actor_schema", &[]).is_err());
        assert!(ConnectorPermissions::scoped("rows.select", vec!["s".into(), "é".repeat(32)], false).validate_local(LocalService::Postgres).is_err());
        assert!(ConnectorPermissions::scoped("sql.raw", vec![], true).validate_local(LocalService::Postgres).is_err());
        assert!(ConnectorPermissions::scoped("secrets.delete", vec![], true).validate_local(LocalService::Vault).is_err());
    }
}

pub fn parent(service: LocalService, schema: &str) -> ConnectorPermissions {
    let mut permissions = ConnectorPermissions::local_preset(service, PermissionPreset::ReadWrite,
        if service == LocalService::Postgres { vec![schema.into()] } else { Vec::new() });
    if service == LocalService::Postgres {
        permissions.scopes.push(ConnectorScope { operation: "rows.delete".into(), root: vec![schema.into()], descendants: true });
    }
    permissions
}

pub fn organization(policy: &EffectPolicy, service: LocalService) -> ConnectorPermissions {
    if !policy.effects.iter().any(|effect| effect == service.effect()) { return ConnectorPermissions::deny_all(); }
    // The org document is shared by all compute actors; the independent actor
    // ceiling confines its schema. Never overwrite this with one user's schema.
    policy.connector_ceilings.get(service.id()).cloned().unwrap_or_else(||
        ConnectorPermissions::local_preset(service, PermissionPreset::ReadWrite, Vec::new()))
}

pub fn declared(service: LocalService, config: &CellConfig, kind: CellType,
    schema: &str, secrets: &[String]) -> Result<ConnectorPermissions, String> {
    let parent = parent(service, schema);
    let mut requested = match config.connectors.iter().find(|grant| grant.connection == service.selection()) {
        Some(grant) => grant.permissions.clone().unwrap_or_else(|| parent.clone()),
        None if service == LocalService::Postgres && kind == CellType::DbSink =>
            ConnectorPermissions::scoped("rows.insert", vec![schema.into(), config.table.clone().ok_or("database sink has no table")?], false),
        None if service == LocalService::Postgres => ConnectorPermissions::local_preset(service, PermissionPreset::ReadOnly, vec![schema.into()]),
        None => ConnectorPermissions { scopes: secrets.iter().flat_map(|name|
            ["secrets.read", "secrets.describe"].map(|operation| ConnectorScope {
                operation: operation.into(), root: vec![name.clone()], descendants: false }))
            .chain(std::iter::once(ConnectorScope { operation: "secrets.list".into(), root: Vec::new(), descendants: false })).collect(),
            max_request_bytes: 1_048_576, max_response_bytes: 16_777_216 },
    };
    if service == LocalService::Postgres && kind == CellType::DbSink {
        requested = requested.intersect(&ConnectorPermissions::scoped("rows.insert",
            vec![schema.into(), config.table.clone().ok_or("database sink has no table")?], false));
    }
    requested.validate_local(service)?;
    parent.narrow(&requested)
}

/// Refreshes reload the actor membership, cell declaration and secret-name
/// inventory under the same org lock as edits/revocation. No stale DTO can mint
/// a wider local cell after an acknowledged policy/declaration change.
pub async fn mint(org: &Org, user: &User, cell: &Cell, service: LocalService,
    requested: &ConnectorPermissions) -> Result<Value, ServerFnError> {
    requested.validate_local(service).map_err(errors::bad_request)?;
    let initial = super::db::org_settings(org).await?.effect_policy.validate().map_err(errors::forbidden)?;
    if !initial.effects.iter().any(|effect| effect == service.effect()) {
        return Err(errors::forbidden("organization local-service effect denied"));
    }
    let key = warrant::root_key().map_err(errors::unavailable)?;
    let schema = if service == LocalService::Postgres {
        compute::ensure(&org.id, &org.slug, &user.id).await.map_err(errors::bad_gateway)?
    } else { String::new() };
    let mut tx = connector::lock(&org.id).await?;
    let row = sqlx::query("select c.config::text as config, c.kind, c.variant, g.id::text as graph from graph_cells c \
        join graphs g on g.id = c.graph_id join projects p on p.id = g.project_id \
        join memberships m on m.org_id = p.org_id and m.user_id = $3::uuid join users u on u.id = m.user_id \
        where c.id = $1::uuid and p.org_id = $2::uuid and u.deleted_at is null")
        .bind(&cell.id).bind(&org.id).bind(&user.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?
        .ok_or_else(|| errors::forbidden("local-service cell or actor membership is no longer available"))?;
    let graph: String = row.get("graph");
    let kind = CellType::from_db(&row.get::<String, _>("kind"), row.get::<Option<String>, _>("variant").as_deref())
        .ok_or_else(|| errors::forbidden("invalid local-service cell type"))?;
    let config: CellConfig = serde_json::from_str(&row.get::<String, _>("config"))
        .map_err(|_| errors::forbidden("invalid local-service declaration"))?;
    let names = sqlx::query("select config ->> 'name' as name from graph_cells where graph_id = $1::uuid \
        and kind = 'source' and variant = 'secret' and config ? 'set_at'")
        .bind(&graph).fetch_all(&mut *tx).await.map_err(errors::db_error)?;
    let secrets: Vec<String> = names.iter().filter_map(|row| row.get::<Option<String>, _>("name")).collect();
    let parent = parent(service, &schema);
    let current = declared(service, &config, kind, &schema, &secrets).map_err(errors::forbidden)?;
    current.narrow(requested).map_err(errors::forbidden)?;
    let policy = connector::policy(&mut tx, &org.id).await?;
    let organization = organization(&policy, service);
    let effective = requested.intersect(&organization);
    let provider = service.id();
    let connection = connection(service, &graph);
    let account = format!("{}/{}", user.id, connection);
    vault::write(&format!("thirdparty/{provider}/{}/{connection}/permissions", user.id), &json!(parent))
        .await.map_err(|_| errors::bad_gateway("could not provision the actor-bound local-service ceiling"))?;
    connector::publish_ceiling(&org.id, provider, connection, &organization).await?;
    let mut tokens = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    let operations: std::collections::BTreeSet<_> = effective.scopes.iter().map(|scope| scope.operation.clone()).collect();
    for operation in operations {
        let grant = warrant::for_connection(&key, &org.id, provider, &operation, connection, 0);
        let warrant_permissions = ConnectorPermissions { scopes: effective.scopes.iter().filter(|scope| scope.operation == operation).cloned().collect(), ..effective.clone() };
        let projection = json!({"account": account, "cell": effective.named_capability_json(provider, connection),
            "warrant": warrant_permissions.named_capability_json(provider, connection)});
        sqlx::query("insert into connector_authorities (warrant_id, org_id, connection_ref, owner_id, run_id, account, provider, graph_id, cell_id, expires_at, projection) \
            values ($1::uuid,$2::uuid,$3,$4::uuid,$5::uuid,$6,$7,$8::uuid,$9::uuid,to_timestamp($10::bigint),$11::jsonb)")
            .bind(&grant.warrant.id).bind(&org.id).bind(connection).bind(&user.id).bind(&grant.run_id)
            .bind(&account).bind(provider).bind(&graph).bind(&cell.id).bind((grant.now + warrant::LIFETIME_SECS) as i64)
            .bind(projection.to_string())
            .execute(&mut *tx).await.map_err(errors::db_error)?;
        let path = connector::authority_path(&org.id, &grant.run_id, &grant.warrant.id);
        if vault::write(&path, &projection).await.is_err() {
            for path in paths { let _ = vault::delete(&path).await; }
            return Err(errors::bad_gateway("could not provision local-service run authority"));
        }
        paths.push(path);
        tokens.push(json!({"operation": operation, "cost": 0, "warrant": grant.warrant.to_json()}));
    }
    if let Err(error) = tx.commit().await {
        for path in paths { let _ = vault::delete(&path).await; }
        return Err(errors::db_error(error));
    }
    Ok(json!({"provider": provider, "connection": connection, "account": account,
        "organization": organization.named_capability_json(provider, connection),
        "connectionPermissions": parent.named_capability_json(provider, connection),
        "cell": effective.named_capability_json(provider, connection),
        "warrantPermissions": effective.named_capability_json(provider, connection), "warrants": tokens}))
}
