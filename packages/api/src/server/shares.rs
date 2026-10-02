//! Public links delegate immutable view/input access, not credentials or owner
//! editing authority. Every browser has its own ephemeral runner session. Lun
//! consumes a checked Trace/Error-only execution envelope on start and update.
use dioxus::{prelude::ServerFnError, fullstack::{FullstackContext, HeaderValue}};
use serde_json::{json, Value};
use sqlx::Row;
use crate::{CellType, ShareCreated, SharedCell, SharedNotebook};
use super::{db, errors, graphs, lun, session};

/// Public responses contain only outcomes attached to the published cells.
/// Filtering the cell list alone must not leak a hidden secret's runtime node.
fn public_nodes(cells: &[SharedCell], nodes: Vec<crate::GraphNode>) -> Vec<crate::GraphNode> {
    nodes.into_iter().filter(|node| cells.iter().any(|cell| cell.node_id == Some(node.id))).collect()
}

async fn authorize(org: &crate::Org, user: &crate::User, project: &str, graph: &str) -> Result<crate::GraphDetail, ServerFnError> {
    let detail = graphs::detail(org, user, project, graph).await?;
    let creator = sqlx::query("select created_by::text as creator from graphs where id=$1::uuid")
        .bind(&detail.graph.id).fetch_one(db::pool()?).await.map_err(errors::db_error)?.get::<Option<String>,_>("creator");
    if !matches!(org.role.as_str(), "owner" | "admin") && creator.as_deref() != Some(&user.id) {
        return Err(errors::forbidden("only the notebook creator or an organization admin can publish or revoke public shares"));
    }
    Ok(detail)
}

pub async fn create(org: &crate::Org, user: &crate::User, project: &str, graph: &str) -> Result<ShareCreated, ServerFnError> {
    let d = authorize(org, user, project, graph).await?;
    if d.graph.status != "ready" || d.cells.iter().any(|c| c.stale) {
        return Err(errors::conflict("build the current notebook before creating an immutable public share"));
    }
    let build = sqlx::query("select session_build_id from graphs where id=$1::uuid").bind(&d.graph.id)
        .fetch_one(db::pool()?).await.map_err(errors::db_error)?.get::<Option<String>,_>("session_build_id")
        .ok_or_else(|| errors::conflict("notebook has no ready runtime build"))?;
    let mut inputs = std::collections::BTreeMap::new();
    let cells: Vec<SharedCell> = d.cells.iter().filter(|c| c.cell_type != CellType::Secret).map(|c| {
        let input = (c.cell_type == CellType::UiInput).then(|| c.config.input.clone().unwrap_or_else(|| c.name.clone()));
        if c.cell_type.is_source() { if let Some(value) = &c.last_input { inputs.insert(c.config.input.clone().unwrap_or_else(|| c.name.clone()), value.clone()); } }
        let node = crate::node_of(&d.nodes, c);
        SharedCell { id: c.id.clone(), name: c.name.clone(), description: c.description.clone(), input,
            input_type: c.implementation.as_ref().and_then(|i| i.input_type.clone()).or(c.config.output_type.clone()),
            choices: c.config.choices.clone(), node_id: node.map(|n| n.id), renderer: (c.cell_type == CellType::UiSink).then(|| c.config.format.clone().unwrap_or_else(|| "table".into())) }
    }).collect();
    let nodes = public_nodes(&cells, d.nodes);
    let snapshot = SharedNotebook { name: d.graph.name, cells, nodes, inputs: inputs.clone() };
    let token = session::random_token(32)?;
    let id: String = sqlx::query("insert into notebook_shares(graph_id,owner_id,token_hash,build_id,snapshot,initial_inputs) values($1::uuid,$2::uuid,$3,$4,$5::jsonb,$6::jsonb) returning id::text as id")
        .bind(&d.graph.id).bind(&user.id).bind(session::hash(&token)).bind(build).bind(serde_json::to_string(&snapshot).unwrap())
        .bind(serde_json::to_string(&inputs).unwrap()).fetch_one(db::pool()?).await.map_err(errors::db_error)?.get("id");
    let origin = super::config::public_url(&session::request_headers().await?);
    Ok(ShareCreated { id, url: format!("{origin}/s/{token}") })
}

pub async fn revoke(org: &crate::Org, user: &crate::User, project: &str, graph: &str, id: &str) -> Result<(), ServerFnError> {
    let detail = authorize(org, user, project, graph).await?;
    let mut tx = db::pool()?.begin().await.map_err(errors::db_error)?;
    // Stop new visitors via the FK parent lock, then wait for any in-flight
    // viewer update before collecting its final runtime session for teardown.
    sqlx::query("select id from notebook_shares where id::text=$1 and graph_id=$2::uuid for update")
        .bind(id).bind(&detail.graph.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?;
    let sessions = sqlx::query("select s.lun_session_id from notebook_share_sessions s join notebook_shares sh on sh.id=s.share_id where sh.id::text=$1 and sh.graph_id=$2::uuid for update of s")
        .bind(id).bind(&detail.graph.id).fetch_all(&mut *tx).await.map_err(errors::db_error)?;
    sqlx::query("delete from notebook_shares where id::text=$1 and graph_id=$2::uuid").bind(id).bind(&detail.graph.id)
        .execute(&mut *tx).await.map_err(errors::db_error)?;
    tx.commit().await.map_err(errors::db_error)?;
    for row in sessions { if let Some(id) = row.get::<Option<String>,_>("lun_session_id") { lun::end(&id).await; } }
    Ok(())
}

pub async fn list(org: &crate::Org, user: &crate::User, project: &str, graph: &str) -> Result<Vec<crate::ShareInfo>, ServerFnError> {
    let detail = authorize(org, user, project, graph).await?;
    let rows = sqlx::query("select id::text as id,created_at::text as at from notebook_shares where graph_id=$1::uuid order by created_at desc")
        .bind(detail.graph.id).fetch_all(db::pool()?).await.map_err(errors::db_error)?;
    Ok(rows.into_iter().map(|row| crate::ShareInfo { id: row.get("id"), created_at: row.get("at") }).collect())
}

struct Share {
    id: String, org: String, user: String, graph: String, build: String, snapshot: SharedNotebook, effects: Vec<String>,
}

async fn lookup(token: &str) -> Result<Share, ServerFnError> {
    if token.len() != 43 || !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return Err(errors::not_found("no such public notebook")); }
    let row = sqlx::query("select sh.id::text as id, sh.owner_id::text as owner, sh.graph_id::text as graph, p.org_id::text as org, sh.build_id, sh.snapshot::text as snapshot, o.effect_policy::text as policy \
        from notebook_shares sh join graphs g on g.id=sh.graph_id join projects p on p.id=g.project_id join orgs o on o.id=p.org_id \
        join memberships m on m.org_id=p.org_id and m.user_id=sh.owner_id join users u on u.id=sh.owner_id where sh.token_hash=$1 and u.deleted_at is null")
        .bind(session::hash(token)).fetch_optional(db::pool()?).await.map_err(errors::db_error)?.ok_or_else(|| errors::not_found("public notebook was revoked or its owner no longer belongs to the organization"))?;
    let policy = row.get::<Option<String>,_>("policy").map(|p| serde_json::from_str::<crate::EffectPolicy>(&p)).transpose()
        .map_err(|_| errors::forbidden("invalid organization policy"))?.unwrap_or_default();
    let effects = policy.validate().map_err(errors::forbidden)?.effects.into_iter().filter(|e| matches!(e.as_str(), "Trace" | "Error")).collect();
    Ok(Share { id: row.get("id"), org: row.get("org"), user: row.get("owner"), graph: row.get("graph"), build: row.get("build_id"),
        snapshot: serde_json::from_str(&row.get::<String,_>("snapshot")).map_err(|_| errors::bad_gateway("invalid public snapshot"))?, effects })
}

async fn viewer(share: &Share) -> Result<Vec<u8>, ServerFnError> {
    let name = format!("tn_share_{}", share.id.replace('-', ""));
    let headers = session::request_headers().await?;
    if let Some(token) = headers.get("cookie").and_then(|h| h.to_str().ok()).and_then(|cookies| cookies.split(';').find_map(|c| c.trim().strip_prefix(&format!("{name}=")))) {
        if token.len() == 43 && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            let hash = session::hash(token);
            if sqlx::query("select 1 from notebook_share_sessions where id_hash=$1 and share_id=$2::uuid and expires_at>now()")
                .bind(&hash).bind(&share.id).fetch_optional(db::pool()?).await.map_err(errors::db_error)?.is_some() { return Ok(hash); }
        }
    }
    let token = session::random_token(32)?;
    let hash = session::hash(&token);
    let mut tx = db::pool()?.begin().await.map_err(errors::db_error)?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 1))").bind(&share.id).execute(&mut *tx).await.map_err(errors::db_error)?;
    let count: i64 = sqlx::query("select count(*) as n from notebook_share_sessions where share_id=$1::uuid and expires_at>now()")
        .bind(&share.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?.get("n");
    if count >= 200 { return Err(errors::forbidden("public notebook visitor limit reached; try again after a session expires")); }
    sqlx::query("insert into notebook_share_sessions(id_hash,share_id,inputs,nodes,expires_at) values($1,$2::uuid,$3::jsonb,$4::jsonb,now()+interval '30 minutes')")
        .bind(&hash).bind(&share.id).bind(serde_json::to_string(&share.snapshot.inputs).unwrap()).bind(serde_json::to_string(&share.snapshot.nodes).unwrap())
        .execute(&mut *tx).await.map_err(errors::db_error)?;
    tx.commit().await.map_err(errors::db_error)?;
    if let Some(context) = FullstackContext::current() {
        let secure = super::config::public_url(&headers).starts_with("https://");
        let cookie = format!("{name}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=1800{}", if secure { "; Secure" } else { "" });
        context.add_response_header(dioxus::fullstack::http::header::SET_COOKIE, HeaderValue::from_str(&cookie).map_err(|_| errors::internal("invalid share cookie"))?);
    }
    Ok(hash)
}

pub async fn get(token: &str) -> Result<SharedNotebook, ServerFnError> {
    let share = lookup(token).await?;
    let viewer = viewer(&share).await?;
    let row = sqlx::query("select inputs::text as inputs, nodes::text as nodes from notebook_share_sessions where id_hash=$1 and share_id=$2::uuid")
        .bind(viewer).bind(&share.id).fetch_one(db::pool()?).await.map_err(errors::db_error)?;
    let mut snapshot = share.snapshot;
    snapshot.inputs = serde_json::from_str(&row.get::<String,_>("inputs")).map_err(|_| errors::bad_gateway("invalid viewer inputs"))?;
    snapshot.nodes = public_nodes(&snapshot.cells, serde_json::from_str(&row.get::<String,_>("nodes")).map_err(|_| errors::bad_gateway("invalid viewer outcomes"))?);
    Ok(snapshot)
}

pub async fn feed(token: &str, cell: &str, value: Value) -> Result<SharedNotebook, ServerFnError> {
    if value.to_string().len() > 1_048_576 { return Err(errors::bad_request("shared input exceeds 1 MiB")); }
    let share = lookup(token).await?;
    let input = share.snapshot.cells.iter().find(|c| c.id == cell).and_then(|c| c.input.clone()).ok_or_else(|| errors::forbidden("public links permit only declared UI inputs"))?;
    let viewer = viewer(&share).await?;
    let mut tx = db::pool()?.begin().await.map_err(errors::db_error)?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 1))").bind(&share.id).execute(&mut *tx).await.map_err(errors::db_error)?;
    let count: i64 = sqlx::query("select count(*) as n from notebook_share_calls where share_id=$1::uuid and at>now()-interval '1 minute'")
        .bind(&share.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?.get("n");
    if count >= 60 { return Err(errors::forbidden("public notebook input rate limit reached")); }
    sqlx::query("insert into notebook_share_calls(share_id) values($1::uuid)").bind(&share.id).execute(&mut *tx).await.map_err(errors::db_error)?;
    // Charge the attempt independently: a malformed input or unavailable
    // runtime must not roll back its rate-limit slot. Other viewers need not
    // wait under the share-wide lock while a runtime computation executes.
    tx.commit().await.map_err(errors::db_error)?;
    let mut tx = db::pool()?.begin().await.map_err(errors::db_error)?;
    let row = sqlx::query("select inputs::text as inputs,lun_session_id from notebook_share_sessions where id_hash=$1 and share_id=$2::uuid and expires_at>now() for update")
        .bind(&viewer).bind(&share.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?;
    let mut inputs: serde_json::Map<String,Value> = serde_json::from_str(&row.get::<String,_>("inputs")).map_err(|_| errors::bad_gateway("invalid viewer inputs"))?;
    inputs.insert(input.clone(), value.clone());
    let execution = json!({"safeShare":true,"binding":{"org_id":share.org,"user_id":share.user,"graph_id":share.graph},
        "policy":{"effects":share.effects,"domains":[]},"connectors":{}});
    let update = json!({"inputs":{input:value},"policy":execution["policy"],"binding":execution["binding"],"connectors":{}});
    let answer = match row.get::<Option<String>,_>("lun_session_id") {
        Some(id) => match lun::update(&id, &update).await.map_err(errors::bad_gateway)? { Some(answer) => answer, None => {
            let mut body = execution.clone(); body["inputs"] = json!(inputs);
            lun::start(&share.build, "main", &body).await.map_err(errors::bad_gateway)?
        } },
        None => { let mut body = execution; body["inputs"] = json!(inputs); lun::start(&share.build, "main", &body).await.map_err(errors::bad_gateway)? },
    };
    let nodes = public_nodes(&share.snapshot.cells, answer.nodes);
    sqlx::query("update notebook_share_sessions set inputs=$2::jsonb,nodes=$3::jsonb,lun_session_id=$4 where id_hash=$1")
        .bind(viewer).bind(serde_json::to_string(&inputs).unwrap()).bind(serde_json::to_string(&nodes).unwrap()).bind(answer.session)
        .execute(&mut *tx).await.map_err(errors::db_error)?;
    tx.commit().await.map_err(errors::db_error)?;
    let mut snapshot = share.snapshot; snapshot.inputs = inputs.into_iter().collect(); snapshot.nodes = nodes;
    Ok(snapshot)
}

pub async fn cleanup() {
    let Ok(pool) = db::pool() else { return; };
    let Ok(rows) = sqlx::query("delete from notebook_share_sessions where expires_at<=now() returning lun_session_id").fetch_all(pool).await else { return; };
    for row in rows { if let Some(id) = row.get::<Option<String>,_>("lun_session_id") { lun::end(&id).await; } }
    let _ = sqlx::query("delete from notebook_share_calls where at<now()-interval '1 hour'").execute(pool).await;
}
