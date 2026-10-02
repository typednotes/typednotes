//! Destructive lifecycle changes share the minting lock. Revoke effects and
//! remove external resources before committing relational cascades. Cleanup is
//! idempotent: a failed distributed step leaves SQL rows available for retry.
use dioxus::prelude::ServerFnError;
use sqlx::{Postgres, Row, Transaction};

use crate::{Org, Provider, User};
use super::{compute, connector, db, errors, graphs, lode, lun, vault};

type Tx = Transaction<'static, Postgres>;

async fn begin() -> Result<Tx, ServerFnError> {
    let mut tx = db::pool()?.begin().await.map_err(errors::db_error)?;
    // A single lock also orders multi-org account deletions against each other.
    sqlx::query("select pg_advisory_xact_lock(hashtextextended('typednotes:tenant-deletion', 2))")
        .execute(&mut *tx).await.map_err(errors::db_error)?;
    let present = sqlx::query("select to_regprocedure('tenant_deletion_ready()') is not null as ready")
        .fetch_one(&mut *tx).await.map_err(errors::db_error)?.get::<bool, _>("ready");
    if !present || !sqlx::query("select tenant_deletion_ready() as ready")
        .fetch_one(&mut *tx).await.map_err(errors::db_error)?.get::<bool, _>("ready") {
        return Err(errors::unavailable("deletion requires app migration 0012 and ledger migration 0003; no resources were removed"));
    }
    Ok(tx)
}

async fn lock_org(tx: &mut Tx, org: &str) -> Result<(), ServerFnError> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(org).execute(&mut **tx).await.map_err(errors::db_error)?;
    sqlx::query("select id from orgs where id=$1::uuid for update")
        .bind(org).fetch_optional(&mut **tx).await.map_err(errors::db_error)?;
    Ok(())
}

async fn delete_secret(path: &str) -> Result<(), ServerFnError> {
    vault::delete(path).await.map_err(|_| errors::bad_gateway(
        "external credential cleanup failed; database rows were retained so deletion can be retried"))
}

/// Current actor-bound authorities are not represented separately from their
/// credential owners in older mint records. Revoke all affected org projections
/// and reset its live sessions; surviving notebooks re-register fresh authority.
async fn quiesce(tx: &mut Tx, org: &str) -> Result<(), ServerFnError> {
    let gates = sqlx::query("select provider, id::text as connection from connections where org_id=$1::uuid \
        union select provider, connection_ref from connector_authorities where org_id=$1::uuid")
        .bind(org).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for gate in gates {
        connector::publish_ceiling(org, &gate.get::<String, _>("provider"), &gate.get::<String, _>("connection"), &crate::ConnectorPermissions::deny_all()).await?;
    }
    connector::revoke(tx, org, None).await?;
    // The parent locks also prevent new graph/share session insertions while
    // teardown collects the final ids of any already-running visitor updates.
    sqlx::query("select id from projects where org_id=$1::uuid for update")
        .bind(org).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    let rows = sqlx::query("select g.id::text as id, g.lode_session_id, g.lun_session_id from graphs g \
        join projects p on p.id=g.project_id where p.org_id=$1::uuid for update of g")
        .bind(org).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for row in rows {
        if let Some(id) = row.get::<Option<String>, _>("lode_session_id") {
            lode::narrow(&id, &[]).await.map_err(errors::bad_gateway)?;
            lode::abort(&id).await.map_err(errors::bad_gateway)?;
        }
        if let Some(id) = row.get::<Option<String>, _>("lun_session_id") {
            lun::end_checked(&id).await.map_err(errors::bad_gateway)?;
        }
    }
    sqlx::query("update graphs g set lode_session_id=null, lun_session_id=null, session_user_id=null, \
        status=case when g.status='implementing' then 'editing' else g.status end, updated_at=now() \
        from projects p where g.project_id=p.id and p.org_id=$1::uuid")
        .bind(org).execute(&mut **tx).await.map_err(errors::db_error)?;
    sqlx::query("update graph_cells c set writing=false from graphs g, projects p \
        where c.graph_id=g.id and g.project_id=p.id and p.org_id=$1::uuid")
        .bind(org).execute(&mut **tx).await.map_err(errors::db_error)?;
    Ok(())
}

async fn project_resources(tx: &mut Tx, org: &str, owner: Option<&str>) -> Result<(), ServerFnError> {
    let shares = sqlx::query("select sh.id::text as id from notebook_shares sh join graphs g on g.id=sh.graph_id \
        join projects p on p.id=g.project_id where p.org_id=$1::uuid \
        and ($2::text is null or p.created_by::text=$2 or sh.owner_id::text=$2) for update of sh")
        .bind(org).bind(owner).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for share in shares {
        let id: String = share.get("id");
        let sessions = sqlx::query("select lun_session_id from notebook_share_sessions where share_id=$1::uuid for update")
            .bind(&id).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
        for session in sessions {
            if let Some(id) = session.get::<Option<String>, _>("lun_session_id") {
                lun::end_checked(&id).await.map_err(errors::bad_gateway)?;
            }
        }
        sqlx::query("delete from notebook_shares where id=$1::uuid").bind(id)
            .execute(&mut **tx).await.map_err(errors::db_error)?;
    }
    let secrets = sqlx::query("select g.id::text as graph, c.config->>'name' as name from graph_cells c \
        join graphs g on g.id=c.graph_id join projects p on p.id=g.project_id \
        where p.org_id=$1::uuid and ($2::text is null or p.created_by::text=$2) \
        and c.variant='secret' and c.config ? 'set_at'")
        .bind(org).bind(owner).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for secret in secrets {
        let name = secret.get::<Option<String>, _>("name")
            .ok_or_else(|| errors::bad_gateway("secret cell has no name; refusing incomplete cleanup"))?;
        delete_secret(&graphs::secret_path(org, &secret.get::<String, _>("graph"), &name)).await?;
    }
    Ok(())
}

async fn private_resources(tx: &mut Tx, org: &str, owner: Option<&str>) -> Result<(), ServerFnError> {
    let rows = sqlx::query("select id::text as id, user_id::text as owner, provider from connections \
        where org_id=$1::uuid and ($2::text is null or user_id::text=$2)")
        .bind(org).bind(owner).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for row in rows {
        let provider_id: String = row.get("provider");
        let provider = Provider::from_id(&provider_id)
            .ok_or_else(|| errors::bad_gateway("unknown connection provider; refusing incomplete credential cleanup"))?;
        let id: String = row.get("id");
        let path = vault::credential_path(provider, &row.get::<String, _>("owner"), &id);
        delete_secret(&path).await?;
        delete_secret(&format!("{path}/permissions")).await?;
        delete_secret(&connector::organization_path(org, &provider_id, &id)).await?;
    }
    let schemas = sqlx::query("select user_id::text as owner, name from compute_schemas \
        where org_id=$1::uuid and ($2::text is null or user_id::text=$2)")
        .bind(org).bind(owner).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    for schema in schemas {
        delete_secret(&compute::credential_path(org, &schema.get::<String, _>("owner"))).await?;
        compute::drop_schema(&schema.get::<String, _>("name")).await.map_err(errors::bad_gateway)?;
    }
    Ok(())
}

pub async fn organization(org: &Org, user: &User, confirm: &str) -> Result<(), ServerFnError> {
    if org.role != "owner" { return Err(errors::forbidden("only the org's owners can delete it")); }
    if confirm.trim() != org.slug { return Err(errors::bad_request(format!("type the org's slug, {}, to confirm", org.slug))); }
    let mut tx = begin().await?;
    lock_org(&mut tx, &org.id).await?;
    let owner = sqlx::query("select 1 from memberships m join users u on u.id=m.user_id \
        where m.org_id=$1::uuid and m.user_id=$2::uuid and m.role='owner' and u.deleted_at is null")
        .bind(&org.id).bind(&user.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?;
    if owner.is_none() { return Err(errors::forbidden("organization ownership changed; reload before deleting")); }
    quiesce(&mut tx, &org.id).await?;
    project_resources(&mut tx, &org.id, None).await?;
    private_resources(&mut tx, &org.id, None).await?;
    sqlx::query("delete from orgs where id=$1::uuid").bind(&org.id)
        .execute(&mut *tx).await.map_err(errors::db_error)?;
    tx.commit().await.map_err(errors::db_error)
}

async fn user_orgs(tx: &mut Tx, user: &str) -> Result<Vec<String>, ServerFnError> {
    let rows = sqlx::query("select org_id::text as id from memberships where user_id=$1::uuid \
        union select org_id::text from projects where created_by=$1::uuid \
        union select org_id::text from connections where user_id=$1::uuid \
        union select org_id::text from compute_schemas where user_id=$1::uuid \
        union select p.org_id::text from graphs g join projects p on p.id=g.project_id \
            where g.created_by=$1::uuid or g.session_user_id=$1::uuid \
        union select p.org_id::text from notebook_shares sh join graphs g on g.id=sh.graph_id \
            join projects p on p.id=g.project_id where sh.owner_id=$1::uuid \
        union select org_id::text from connector_authorities where owner_id=$1::uuid order by id")
        .bind(user).fetch_all(&mut **tx).await.map_err(errors::db_error)?;
    Ok(rows.into_iter().map(|row| row.get("id")).collect())
}

pub async fn account(user: &User, confirm: &str) -> Result<(), ServerFnError> {
    if !confirm.trim().eq_ignore_ascii_case(&user.email) {
        return Err(errors::bad_request("type your email address to confirm account deletion"));
    }
    let mut tx = begin().await?;
    let orgs = user_orgs(&mut tx, &user.id).await?;
    for org in &orgs { lock_org(&mut tx, org).await?; }
    let current = sqlx::query("select id from users where id=$1::uuid and deleted_at is null for update")
        .bind(&user.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?;
    if current.is_none() { return Err(errors::unauthorized()); }
    // Newly committed membership/ownership while locks were being acquired is
    // a retry, never permission to skip that organization's external resources.
    if user_orgs(&mut tx, &user.id).await? != orgs {
        return Err(errors::conflict("your workspaces changed during deletion; retry"));
    }
    let blocked = sqlx::query("select o.name from memberships mine join orgs o on o.id=mine.org_id \
        where mine.user_id=$1::uuid and mine.role='owner' \
        and exists(select 1 from memberships m where m.org_id=mine.org_id and m.user_id<>$1::uuid) \
        and not exists(select 1 from memberships m where m.org_id=mine.org_id and m.user_id<>$1::uuid and m.role='owner')")
        .bind(&user.id).fetch_all(&mut *tx).await.map_err(errors::db_error)?;
    if !blocked.is_empty() {
        let names = blocked.iter().map(|row| row.get::<String, _>("name")).collect::<Vec<_>>().join(", ");
        return Err(errors::conflict(format!("transfer ownership before deleting your account: {names}")));
    }
    for org in &orgs {
        let remaining: i64 = sqlx::query("select count(*) as n from memberships where org_id=$1::uuid and user_id<>$2::uuid")
            .bind(org).bind(&user.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?.get("n");
        quiesce(&mut tx, org).await?;
        let owner = (remaining != 0).then_some(user.id.as_str());
        project_resources(&mut tx, org, owner).await?;
        private_resources(&mut tx, org, owner).await?;
        if remaining == 0 {
            sqlx::query("delete from orgs where id=$1::uuid").bind(org)
                .execute(&mut *tx).await.map_err(errors::db_error)?;
        } else {
            // Remove parents before the user FK's sibling SET NULL actions.
            // Otherwise PostgreSQL can update a graph/message whose project
            // or channel is already disappearing along another cascade path.
            sqlx::query("delete from projects where org_id=$1::uuid and created_by=$2::uuid")
                .bind(org).bind(&user.id).execute(&mut *tx).await.map_err(errors::db_error)?;
            sqlx::query("delete from connections where org_id=$1::uuid and user_id=$2::uuid")
                .bind(org).bind(&user.id).execute(&mut *tx).await.map_err(errors::db_error)?;
            sqlx::query("delete from compute_schemas where org_id=$1::uuid and user_id=$2::uuid")
                .bind(org).bind(&user.id).execute(&mut *tx).await.map_err(errors::db_error)?;
        }
    }
    sqlx::query("delete from users where id=$1::uuid").bind(&user.id)
        .execute(&mut *tx).await.map_err(errors::db_error)?;
    tx.commit().await.map_err(errors::db_error)
}
