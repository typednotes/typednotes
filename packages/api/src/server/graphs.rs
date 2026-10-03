//! Notebooks (docs/computations.md): graphs of prose cells, implemented by
//! `lode`, built and run by `lun`, fed by the app.
//!
//! The app holds the only state that cannot be reconstructed — the cells,
//! and the log of every input it fed (`graph_inputs`) — and drives the three
//! steps of §2:
//!
//! 1. **implement**: a lode session over the project's primary repository,
//!    with a repo `write` warrant, a model warrant and a repo `read` warrant
//!    for lun, all fresh with every message;
//! 2. **build**: once lode's run is over, the published `lun.json` is read
//!    from the repository at lode's commit and submitted to lun;
//! 3. **run**: the graph is registered as a lun session bound to
//!    `(org, user, graph)`, and every input the app feeds — a notebook
//!    widget, a scheduled check, a webhook, a channel message — is recorded,
//!    then fed. A session lun lost is registered again from the record.
//!
//! Every read is scoped by the org the caller was checked to be a member of,
//! then by the project, like projects themselves.

use dioxus::prelude::ServerFnError;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgRow;
use sqlx::Row;

use super::connections::{self, ProviderCall};
use super::db::{pool, slug_check};
use super::errors::{
    bad_gateway, bad_request, conflict, db_error, forbidden, message as err_text, not_found,
    unavailable,
};
use super::lun::{self, Structure};
use super::session::random_token;
use super::{channels, compute, config, lode, projects, vault, warrant};
use crate::{
    build_repair_message, cell_impl, json_fits, lode_message, repair_message, validate_cell,
    validate_org, validate_secret_value, Activity, Cell, CellConfig, CellImpl, CellSaved, CellType,
    Connection, Cron, FeedResult, Graph, GraphDetail, GraphNode, LodeProgress, LunJson, Org,
    Project, Provider, SlugCheck, User, GRAPH_NAME,
};

// ── Rows ────────────────────────────────────────────────────────────────

macro_rules! graph_columns {
    () => {
        "g.id::text as id, g.slug::text as slug, g.name, g.status, g.status_detail, \
         g.commit_sha, g.lun_build_id, g.lun_session_id, g.lode_session_id, g.session_build_id, \
         g.model_connection_id::text as model_connection_id, g.model_name, \
         g.session_user_id::text as session_user_id, g.created_by::text as created_by, \
         g.project_id::text as project_id, g.last_nodes::text as last_nodes, \
         g.structure::text as structure, g.lode_log_start, g.auto_repairs, \
         to_char(g.created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at, \
          to_char(g.updated_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI:SS.US \"UTC\"') as updated_at"
    };
}

/// A graph and what the server keeps next to it.
#[derive(Clone)]
pub struct GraphRow {
    pub graph: Graph,
    pub project_id: String,
    pub lode_session_id: Option<String>,
    pub lun_session_id: Option<String>,
    pub session_user_id: Option<String>,
    pub created_by: Option<String>,
    pub last_nodes: Vec<GraphNode>,
    pub structure: Option<Structure>,
    /// Rewrites the app launched by itself since a member last asked.
    pub auto_repairs: i32,
    /// The ready build the session runs (`graph.build_id` may be a newer
    /// one, building or failed).
    pub session_build_id: Option<String>,
}

fn graph_of(row: &PgRow) -> GraphRow {
    let lun_session_id: Option<String> = row.get("lun_session_id");
    let last_nodes: Option<String> = row.get("last_nodes");
    let structure: Option<String> = row.get("structure");
    GraphRow {
        graph: Graph {
            id: row.get("id"),
            slug: row.get("slug"),
            name: row.get("name"),
            status: row.get("status"),
            detail: row.get("status_detail"),
            commit: row.get("commit_sha"),
            build_id: row.get("lun_build_id"),
            session: lun_session_id.is_some(),
            model_connection_id: row.get("model_connection_id"),
            model_name: row.get("model_name"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            log_start: row.get::<i32, _>("lode_log_start").max(0) as u64,
        },
        auto_repairs: row.get("auto_repairs"),
        session_build_id: row.get("session_build_id"),
        project_id: row.get("project_id"),
        lode_session_id: row.get("lode_session_id"),
        lun_session_id,
        session_user_id: row.get("session_user_id"),
        created_by: row.get("created_by"),
        last_nodes: last_nodes
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        structure: structure
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .map(|v| Structure::from_json(&v)),
    }
}

async fn graph_by_slug(project_id: &str, slug: &str) -> Result<GraphRow, ServerFnError> {
    let row = sqlx::query(concat!(
        "select ",
        graph_columns!(),
        " from graphs g where g.project_id = $1::uuid and g.slug = $2::citext"
    ))
    .bind(project_id)
    .bind(slug.trim())
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("no such notebook"))?;
    Ok(graph_of(&row))
}

async fn graph_by_id(id: &str) -> Result<Option<GraphRow>, ServerFnError> {
    let row = sqlx::query(concat!(
        "select ",
        graph_columns!(),
        " from graphs g where g.id = $1::uuid"
    ))
    .bind(id)
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    Ok(row.as_ref().map(graph_of))
}

macro_rules! cell_columns {
    () => {
        "c.id::text as id, c.position, c.name, c.kind, c.variant, c.description, \
         c.config::text as config, c.impl::text as impl, c.writing, c.issue, c.auto_issue, \
         (c.code_at is not null and c.edited_at > c.code_at) as stale, \
         to_char(c.next_due_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as next_due_at"
    };
}

/// A cell and its stored config, which holds what the client never sees
/// (an endpoint's token hash).
#[derive(Clone)]
pub struct CellRow {
    pub cell: Cell,
    pub raw_config: Value,
    /// The last lun error the app itself sent the cell back to lode for.
    pub auto_issue: Option<String>,
}

fn cell_of(row: &PgRow) -> Option<CellRow> {
    let kind: String = row.get("kind");
    let variant: Option<String> = row.get("variant");
    let cell_type = CellType::from_db(&kind, variant.as_deref())?;
    let raw: String = row.get("config");
    let raw_config: Value = serde_json::from_str(&raw).unwrap_or(json!({}));
    let config: CellConfig = serde_json::from_value(raw_config.clone()).unwrap_or_default();
    let implementation: Option<String> = row.get("impl");
    Some(CellRow {
        cell: Cell {
            id: row.get("id"),
            position: row.get("position"),
            name: row.get("name"),
            cell_type,
            description: row.get("description"),
            implementation: implementation.and_then(|s| serde_json::from_str::<CellImpl>(&s).ok()),
            secret_set: raw_config.get("set_at").is_some(),
            has_endpoint: raw_config.get("token_hash").is_some(),
            config,
            last_input: None,
            next_due_at: row.get("next_due_at"),
            last_check: None,
            writing: row.get("writing"),
            issue: row.get("issue"),
            stale: row.get("stale"),
        },
        raw_config,
        auto_issue: row.get("auto_issue"),
    })
}

pub async fn cells_of(graph_id: &str) -> Result<Vec<CellRow>, ServerFnError> {
    let rows = sqlx::query(concat!(
        "select ",
        cell_columns!(),
        " from graph_cells c where c.graph_id = $1::uuid order by c.position, c.created_at"
    ))
    .bind(graph_id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows.iter().filter_map(cell_of).collect())
}

async fn cell_by_id(graph_id: &str, cell_id: &str) -> Result<CellRow, ServerFnError> {
    let row = sqlx::query(concat!(
        "select ",
        cell_columns!(),
        " from graph_cells c where c.graph_id = $1::uuid and c.id::text = $2"
    ))
    .bind(graph_id)
    .bind(cell_id.trim())
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    row.as_ref()
        .and_then(cell_of)
        .ok_or_else(|| not_found("no such cell"))
}

/// The latest value fed to each input of a graph.
async fn latest_inputs(graph_id: &str) -> Result<Map<String, Value>, ServerFnError> {
    let rows = sqlx::query(
        "select distinct on (input) input, value::text as value from graph_inputs \
         where graph_id = $1::uuid order by input, at desc",
    )
    .bind(graph_id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let value: String = r.get("value");
            Some((r.get("input"), serde_json::from_str(&value).ok()?))
        })
        .collect())
}

// ── Contexts ────────────────────────────────────────────────────────────

/// Everything an operation on a graph needs: its org, project, row, and
/// the user it acts as — the caller in a server function, the session's user
/// on the scheduler's tick or a webhook.
#[derive(Clone)]
pub struct Ctx {
    pub org: Org,
    pub project: Project,
    pub row: GraphRow,
    pub user: User,
}

/// A graph of a project of `org`, for its member `user`.
async fn member_ctx(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
) -> Result<Ctx, ServerFnError> {
    let (project, _) = projects::get(org, project_slug).await?;
    let row = graph_by_slug(&project.id, graph_slug).await?;
    Ok(Ctx {
        org: org.clone(),
        project,
        row,
        user: user.clone(),
    })
}

/// A graph by id, acting as the user its session is bound to (else its
/// creator), for work no request drives.
pub async fn background_ctx(graph_id: &str) -> Result<Ctx, String> {
    let text = |e: ServerFnError| err_text(&e);
    let row = graph_by_id(graph_id)
        .await
        .map_err(text)?
        .ok_or("the notebook is gone")?;
    let user_id = row
        .session_user_id
        .clone()
        .or_else(|| row.created_by.clone())
        .ok_or("the notebook has no user to act as")?;
    let found = sqlx::query(
        "select o.id::text as org_id, o.slug::text as org_slug, o.name as org_name, \
         to_char(o.created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as org_created_at, \
         m.role, p.slug::text as project_slug, \
         u.id::text as user_id, u.email::text as email, u.display_name \
         from projects p join orgs o on o.id = p.org_id \
         join memberships m on m.org_id = o.id and m.user_id = $2::uuid \
         join users u on u.id = m.user_id \
         where p.id = $1::uuid and u.deleted_at is null",
    )
    .bind(&row.project_id)
    .bind(&user_id)
    .fetch_optional(pool().map_err(text)?)
    .await
    .map_err(|e| format!("database error: {e}"))?
    .ok_or("the notebook's user is no longer a member of its org")?;
    let org = Org {
        id: found.get("org_id"),
        slug: found.get("org_slug"),
        name: found.get("org_name"),
        role: found.get("role"),
        created_at: found.get("org_created_at"),
    };
    let project_slug: String = found.get("project_slug");
    let (project, _) = projects::get(&org, &project_slug).await.map_err(text)?;
    Ok(Ctx {
        org,
        project,
        row,
        user: User {
            id: found.get("user_id"),
            email: found.get("email"),
            display_name: found.get("display_name"),
        },
    })
}

async fn reload(ctx: &mut Ctx) -> Result<(), ServerFnError> {
    ctx.row = graph_by_id(&ctx.row.graph.id)
        .await?
        .ok_or_else(|| not_found("no such notebook"))?;
    Ok(())
}

async fn set_status(
    graph_id: &str,
    status: &str,
    detail: Option<&str>,
) -> Result<(), ServerFnError> {
    sqlx::query(
        "update graphs set status = $2, status_detail = $3, updated_at = now() where id = $1::uuid",
    )
    .bind(graph_id)
    .bind(status)
    .bind(detail)
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    Ok(())
}

// ── Graphs ──────────────────────────────────────────────────────────────

pub async fn list(org: &Org, project_slug: &str) -> Result<Vec<Graph>, ServerFnError> {
    let (project, _) = projects::get(org, project_slug).await?;
    let rows = sqlx::query(concat!(
        "select ",
        graph_columns!(),
        " from graphs g where g.project_id = $1::uuid order by g.created_at desc"
    ))
    .bind(&project.id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows.iter().map(|r| graph_of(r).graph).collect())
}

pub async fn check_slug(
    org: &Org,
    project_slug: &str,
    slug: &str,
) -> Result<SlugCheck, ServerFnError> {
    if let Err(message) = crate::validate_slug(slug) {
        return Ok(SlugCheck {
            available: false,
            message,
        });
    }
    let (project, _) = projects::get(org, project_slug).await?;
    let taken =
        sqlx::query("select 1 from graphs where project_id = $1::uuid and slug = $2::citext")
            .bind(&project.id)
            .bind(slug)
            .fetch_optional(pool()?)
            .await
            .map_err(db_error)?
            .is_some();
    Ok(slug_check(slug, taken))
}

pub async fn create(
    org: &Org,
    user: &User,
    project_slug: &str,
    slug: &str,
    name: &str,
) -> Result<Graph, ServerFnError> {
    validate_org(slug, name).map_err(bad_request)?;
    let (project, _) = projects::get(org, project_slug).await?;
    // The org's first AI connection, as a starting point.
    let ai_providers: Vec<String> = Provider::AI
        .iter()
        .copied()
        .filter(|p| p.can_generate())
        .map(|p| p.id().to_string())
        .collect();
    let model = sqlx::query(
        "select id::text as id, provider from connections where org_id = $1::uuid \
         and provider = any($2::text[]) \
         order by created_at limit 1",
    )
    .bind(&org.id)
    .bind(ai_providers)
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    let model_id: Option<String> = model.as_ref().map(|r| r.get("id"));
    let model_name = model
        .as_ref()
        .and_then(|r| Provider::from_id(&r.get::<String, _>("provider")))
        .and_then(default_model);
    let inserted = sqlx::query(concat!(
        "insert into graphs as g (project_id, slug, name, created_by, model_connection_id, model_name) \
         values ($1::uuid, $2, $3, $4::uuid, $5::uuid, $6) returning ",
        graph_columns!()
    ))
    .bind(&project.id)
    .bind(slug)
    .bind(name.trim())
    .bind(&user.id)
    .bind(model_id)
    .bind(model_name)
    .fetch_one(pool()?)
    .await;
    match inserted {
        Ok(row) => Ok(graph_of(&row).graph),
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => Err(conflict(
            format!("this project already has a notebook '{slug}'"),
        )),
        Err(e) => Err(db_error(e)),
    }
}

/// The model a provider's connection defaults to.
pub fn default_model(provider: Provider) -> Option<&'static str> {
    provider.ai_info().and_then(|p| p.default_model)
}

/// Delete a notebook: its creator or an org admin. Its secrets go first, so
/// a vault failure leaves the notebook to retry rather than orphaned values.
pub async fn delete(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
) -> Result<(), ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let admin = org.role == "owner" || org.role == "admin";
    if !admin && ctx.row.created_by.as_deref() != Some(user.id.as_str()) {
        return Err(forbidden(
            "only its creator or an org admin can delete this notebook",
        ));
    }
    for c in cells_of(&ctx.row.graph.id).await? {
        if c.cell.cell_type == CellType::Secret && c.cell.secret_set {
            delete_secret_value(&ctx, &c.cell).await?;
        }
    }
    if let Some(session) = &ctx.row.lun_session_id {
        lun::end(session).await;
    }
    let mut tx = super::connector::lock(&ctx.org.id).await?;
    super::connector::revoke_graph(&mut tx, &ctx.org.id, &ctx.row.graph.id).await?;
    sqlx::query("delete from graphs where id = $1::uuid")
        .bind(&ctx.row.graph.id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(())
}

pub async fn set_model(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    connection_id: &str,
    model_name: &str,
) -> Result<Graph, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let (connection, _) = connections::get(org, user, connection_id.trim()).await?;
    if ctx.row.graph.status == "implementing" {
        return Err(conflict(
            "finish or stop the current implementation before changing its model",
        ));
    }
    if !connection.provider.can_generate() {
        return Err(bad_request(
            "choose a generative AI connection; classifiers do not write code",
        ));
    }
    let name = model_name.trim();
    if !(1..=100).contains(&name.len()) || !name.chars().all(|c| c.is_ascii_graphic()) {
        return Err(bad_request(
            "model: 1–100 characters without spaces, e.g. claude-sonnet-4-5",
        ));
    }
    if connection.provider.ai_api(name) == Some(crate::AiApi::Classifier) {
        return Err(bad_request("choose a generative model; Jev is a classifier"));
    }
    sqlx::query(
        "update graphs set \
          lode_session_id = case when model_connection_id is distinct from $2::uuid \
            or model_name is distinct from $3 then null else lode_session_id end, \
          model_connection_id = $2::uuid, model_name = $3, updated_at = now() \
         where id = $1::uuid",
    )
    .bind(&ctx.row.graph.id)
    .bind(&connection.id)
    .bind(name)
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    Ok(graph_by_id(&ctx.row.graph.id)
        .await?
        .ok_or_else(|| not_found("no such notebook"))?
        .graph)
}

/// The notebook page.
pub async fn detail(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
) -> Result<GraphDetail, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let graph_id = ctx.row.graph.id.clone();
    let inputs = latest_inputs(&graph_id).await?;
    let checks = sqlx::query(
        "select distinct on (s.cell_id) s.cell_id::text as cell_id, s.ok, s.outcome, \
         to_char(s.started_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as at \
         from source_checks s join graph_cells c on c.id = s.cell_id \
         where c.graph_id = $1::uuid order by s.cell_id, s.started_at desc",
    )
    .bind(&graph_id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    let cells = cells_of(&graph_id)
        .await?
        .into_iter()
        .map(|c| {
            let mut cell = c.cell;
            if cell.cell_type.has_input() {
                cell.last_input = cell
                    .config
                    .input
                    .as_ref()
                    .and_then(|i| inputs.get(i))
                    .cloned();
            }
            cell.last_check = checks
                .iter()
                .find(|r| r.get::<String, _>("cell_id") == cell.id)
                .map(|r| {
                    let (at, outcome): (String, String) = (r.get("at"), r.get("outcome"));
                    format!("{at}: {outcome}")
                });
            cell
        })
        .collect();
    let channels = channels::list(org, project_slug).await?;
    let (sources, sinks) = ctx
        .row
        .structure
        .as_ref()
        .map(|s| (s.sources.clone(), s.sinks.clone()))
        .unwrap_or_default();
    Ok(GraphDetail {
        org: ctx.org.clone(),
        project: ctx.project.clone(),
        graph: ctx.row.graph.clone(),
        cells,
        nodes: ctx.row.last_nodes.clone(),
        sources,
        sinks,
        channels,
        activity: activity(&graph_id).await?,
        can_edit: true,
        effect_policy: super::db::org_settings(org).await?.effect_policy,
    })
}

/// The latest updates, checks and deliveries, newest first.
async fn activity(graph_id: &str) -> Result<Vec<Activity>, ServerFnError> {
    let pool = pool()?;
    let mut out: Vec<(String, Activity)> = Vec::new();
    for r in sqlx::query(
        "select to_char(u.at at time zone 'UTC', 'YYYY-MM-DD HH24:MI:SS \"UTC\"') as at, \
         u.fed_by, c.name as cell, jsonb_array_length(u.changed) as n \
         from graph_updates u left join graph_cells c on c.id = u.cell_id \
         where u.graph_id = $1::uuid order by u.at desc limit 15",
    )
    .bind(graph_id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?
    {
        let (at, fed_by, n): (String, String, i32) = (r.get("at"), r.get("fed_by"), r.get("n"));
        let what = if fed_by == "register" {
            "session registered".to_string()
        } else {
            format!("fed by {fed_by}")
        };
        out.push((
            at.clone(),
            Activity {
                at,
                kind: "update".to_string(),
                cell: r.get("cell"),
                ok: true,
                text: format!("{what}: {n} node{} changed", if n == 1 { "" } else { "s" }),
            },
        ));
    }
    for r in sqlx::query(
        "select to_char(s.started_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI:SS \"UTC\"') as at, \
         c.name as cell, s.ok, s.outcome, s.latency_ms \
         from source_checks s join graph_cells c on c.id = s.cell_id \
         where c.graph_id = $1::uuid order by s.started_at desc limit 15",
    )
    .bind(graph_id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?
    {
        let at: String = r.get("at");
        let latency: Option<i32> = r.get("latency_ms");
        let outcome: String = r.get("outcome");
        out.push((
            at.clone(),
            Activity {
                at,
                kind: "check".to_string(),
                cell: r.get("cell"),
                ok: r.get("ok"),
                text: match latency {
                    Some(ms) => format!("{outcome} ({ms} ms)"),
                    None => outcome,
                },
            },
        ));
    }
    for r in sqlx::query(
        "select to_char(e.at at time zone 'UTC', 'YYYY-MM-DD HH24:MI:SS \"UTC\"') as at, \
         c.name as cell, e.ok, e.status, e.fed_at is not null as fed \
         from endpoint_calls e join graph_cells c on c.id = e.cell_id \
         where c.graph_id = $1::uuid order by e.at desc limit 15",
    )
    .bind(graph_id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?
    {
        let at: String = r.get("at");
        let (status, fed): (i32, bool) = (r.get("status"), r.get("fed"));
        out.push((
            at.clone(),
            Activity {
                at,
                kind: "endpoint".to_string(),
                cell: r.get("cell"),
                ok: r.get("ok"),
                text: format!(
                    "delivery answered {status}{}",
                    if fed { ", fed" } else { "" }
                ),
            },
        ));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(out.into_iter().take(25).map(|(_, a)| a).collect())
}

// ── Cells ───────────────────────────────────────────────────────────────

/// The server's half of a cell's validation: what the client cannot know.
async fn check_cell_refs(
    ctx: &Ctx,
    cell_type: CellType,
    config: &CellConfig,
) -> Result<(), ServerFnError> {
    if let Some(channel_id) = &config.channel_id {
        let channels = channels::list(&ctx.org, &ctx.project.slug).await?;
        let channel = channels
            .iter()
            .find(|c| &c.id == channel_id)
            .ok_or_else(|| bad_request("that interface is not one of this project's"))?;
        if cell_type == CellType::ChannelSink && channel.provider != Provider::Slack {
            let recipient = config.recipient.as_deref().unwrap_or("");
            crate::validate_phone(recipient).map_err(|e| bad_request(format!("recipient: {e}")))?;
        }
    }
    if let Some(connection_id) = &config.connection_id {
        let (connection, _) = connections::get(&ctx.org, &ctx.user, connection_id).await?;
        if !Provider::STORAGE.contains(&connection.provider) {
            return Err(bad_request("not a storage connection"));
        }
    }
    let policy = super::db::org_settings(&ctx.org).await?.effect_policy;
    for grant in &config.connectors {
        if let Some(service) = crate::LocalService::from_selection(&grant.connection) {
            if !policy.effects.iter().any(|effect| effect == service.effect()) {
                return Err(forbidden("the organization does not permit this local service"));
            }
            if let Some(permissions) = &grant.permissions {
                let schema = compute::role_name(&ctx.org.slug, &ctx.org.id, &ctx.user.id);
                let permissions = permissions.validate_local(service).map_err(bad_request)?;
                super::local::parent(service, &schema).narrow(&permissions).map_err(bad_request)?;
                if let Some(organization) = policy.connector_ceilings.get(service.id()) {
                    organization.narrow(&permissions).map_err(bad_request)?;
                }
            }
            continue;
        }
        let (connection, _) = connections::get(&ctx.org, &ctx.user, &grant.connection).await?;
        let object_store = matches!(connection.provider, Provider::S3 | Provider::Azure) && policy.effects.iter().any(|effect| effect == "ObjectStore");
        if (!object_store && !policy.effects.iter().any(|effect| effect == "Connector")) || !policy.allows_provider(connection.provider) {
            return Err(forbidden("the organization does not permit this connector"));
        }
        let ceiling = connection.permissions.clone().unwrap_or_else(|| crate::ConnectorPermissions::preset(connection.provider, crate::PermissionPreset::ReadOnly));
        if let Some(requested) = &grant.permissions {
            requested.validate(connection.provider).and_then(|requested| ceiling.narrow(&requested)).map_err(bad_request)?;
            if let Some(organization) = policy.connector_ceilings.get(connection.provider.id()) {
                organization.narrow(requested).map_err(|_| bad_request("a cell cannot widen the organization's connector ceiling"))?;
            }
        }
    }
    if let Some(url) = &config.url {
        let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase)).unwrap_or_default();
        if !policy.effects.iter().any(|e| e == "HTTP") || (!policy.configured_domains && !policy.domains.contains(&host)) {
            return Err(bad_request(format!("the organization has not allowed HTTP access to {host}; update its notebook permissions")));
        }
    }
    if let Some(channel_id) = &config.channel_id {
        let channel = channels::list(&ctx.org, &ctx.project.slug).await?.into_iter().find(|c| c.id == *channel_id).ok_or_else(|| bad_request("no such interface"))?;
        if !policy.allows_provider(channel.provider) { return Err(forbidden("this messaging provider is not allowed by the organization")); }
    }
    if let Some(url) = &config.url {
        check_public_url(url).await.map_err(bad_request)?;
    }
    Ok(())
}

/// Validate named dependencies before storing a declaration. Names are scoped to
/// this notebook; no positional references or references to secret values.
async fn check_declaration(ctx: &Ctx, name: &str, description: &str, config: &mut CellConfig, existing: Option<&str>, cell_type: CellType) -> Result<Vec<Cell>, ServerFnError> {
    let mut cells: Vec<Cell> = cells_of(&ctx.row.graph.id).await?.into_iter().map(|row| row.cell).collect();
    if cells.iter().any(|cell| cell.name == name && Some(cell.id.as_str()) != existing) {
        return Err(conflict(format!("this notebook already has a cell '{name}'")));
    }
    let renamed = cells.iter().find(|cell| Some(cell.id.as_str()) == existing).filter(|cell| cell.name != name)
        .map(|cell| crate::renamed_dependents(&cells, &cell.name, name)).unwrap_or_default();
    for dependent in &renamed {
        crate::validate_description(&dependent.description).map_err(bad_request)?;
        if let Some(cell) = cells.iter_mut().find(|cell| cell.id == dependent.id) { *cell = dependent.clone(); }
    }
    let mut deps = config.dependencies.clone().unwrap_or_default();
    for reference in crate::cell_references(description) {
        if !deps.contains(&reference) { deps.push(reference); }
    }
    config.dependencies = Some(deps);
    let candidate = Cell { id: existing.unwrap_or("new").into(), position: cells.len() as i32,
        name: name.into(), description: description.into(), config: config.clone(), cell_type,
        implementation: None, secret_set: false, has_endpoint: false, last_input: None,
        next_due_at: None, last_check: None, writing: false, issue: None, stale: false };
    if let Some(index) = cells.iter().position(|cell| Some(cell.id.as_str()) == existing) {
        cells[index] = candidate;
    } else { cells.push(candidate); }
    crate::validate_dependencies(&cells).map_err(bad_request)?;
    Ok(renamed.into_iter().filter(|cell| Some(cell.id.as_str()) != existing).collect())
}

/// The stored config: the client's fields, plus what the server keeps
/// (a secret's `set_at`, an endpoint's `token_hash`).
fn stored_config(config: &CellConfig, keep: Option<&Value>) -> Value {
    let mut v = serde_json::to_value(config).unwrap_or(json!({}));
    if let (Some(keep), Some(obj)) = (keep, v.as_object_mut()) {
        for k in ["set_at", "token_hash"] {
            if let Some(x) = keep.get(k) {
                obj.insert(k.to_string(), x.clone());
            }
        }
    }
    v
}

/// The next due time of a scheduled or watch cell, from now.
fn next_due(cell_type: CellType, config: &CellConfig) -> Option<i64> {
    if !matches!(cell_type, CellType::Scheduled | CellType::Watch) {
        return None;
    }
    let cron = Cron::parse(config.schedule.as_deref()?).ok()?;
    cron.next_after(warrant::now() as i64)
}

/// A fresh endpoint token: the URL to show once, and the hash to store.
fn endpoint_token(public_url: &str) -> Result<(String, String), ServerFnError> {
    let token = random_token(32)?;
    let hash = hex::encode(Sha256::digest(token.as_bytes()));
    Ok((format!("{public_url}/hooks/graphs/{token}"), hash))
}

/// What a cell says, as a form submits it.
pub struct CellForm<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub config: &'a CellConfig,
}

pub async fn add_cell(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_type: CellType,
    form: CellForm<'_>,
    public_url: &str,
) -> Result<CellSaved, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let (name, description, mut config) =
        validate_cell(cell_type, form.name, form.description, form.config).map_err(bad_request)?;
    check_declaration(&ctx, &name, &description, &mut config, None, cell_type).await?;
    check_cell_refs(&ctx, cell_type, &config).await?;
    let mut stored = stored_config(&config, None);
    let mut endpoint_url = None;
    if cell_type == CellType::Endpoint {
        let (url, hash) = endpoint_token(public_url)?;
        stored["token_hash"] = json!(hash);
        endpoint_url = Some(url);
    }
    // Declaration insertion, queueing and build adoption share a graph lock.
    // A finishing older build must not mark a newly queued cell ready.
    let mut guard=pool()?.begin().await.map_err(db_error)?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1,3))").bind(&ctx.row.graph.id).execute(&mut *guard).await.map_err(db_error)?;
    super::connector::require_actor(&mut guard,&org.id,&user.id).await?;
    let inserted = sqlx::query(
        "insert into graph_cells (graph_id, position, name, kind, variant, description, config, next_due_at) \
         values ($1::uuid, \
                 (select coalesce(max(position) + 1, 0) from graph_cells where graph_id = $1::uuid), \
                 $2, $3, $4, $5, $6::jsonb, to_timestamp($7::bigint)) \
         returning id::text as id",
    )
    .bind(&ctx.row.graph.id)
    .bind(&name)
    .bind(cell_type.kind())
    .bind(cell_type.variant())
    .bind(&description)
    .bind(stored.to_string())
    .bind(next_due(cell_type, &config))
    .fetch_one(&mut *guard)
    .await;
    let id: String = match inserted {
        Ok(row) => row.get("id"),
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            return Err(conflict(format!(
                "this notebook already has a cell '{name}'"
            )))
        }
        Err(e) => return Err(db_error(e)),
    };
    sqlx::query("update graphs set updated_at=now() where id=$1::uuid").bind(&ctx.row.graph.id).execute(&mut *guard).await.map_err(db_error)?;
    let missing=super::workspace::required(&ctx.project,Some(&ctx.row.graph),&connections::list(org,user).await?,&super::db::org_settings(org).await?.effect_policy);
    let (generation,generation_notice)=if lode::configured()&&missing.is_empty(){
        sqlx::query("insert into graph_generation_requests(graph_id,user_id) values($1::uuid,$2::uuid) \
            on conflict(graph_id) do update set user_id=excluded.user_id,revision=gen_random_uuid(),requested_at=now()")
            .bind(&ctx.row.graph.id).bind(&user.id).execute(&mut *guard).await.map_err(db_error)?;
        sqlx::query("update graphs set status='implementing',status_detail='Preparing repository checkout',updated_at=now() where id=$1::uuid")
            .bind(&ctx.row.graph.id).execute(&mut *guard).await.map_err(db_error)?;
        (true,None)
    }else{
        let notice=if !lode::configured(){"Code writing is not configured on this deployment.".into()}else{missing.join(" ")};
        (false,Some(format!("Cell saved. Finish setup to start code generation: {notice}")))
    };
    guard.commit().await.map_err(db_error)?;
    let generation=if generation{Some(graph_by_id(&ctx.row.graph.id).await?.ok_or_else(||not_found("notebook was removed"))?.graph)}else{None};
    Ok(CellSaved {
        cell: cell_by_id(&ctx.row.graph.id, &id).await?.cell,
        endpoint_url,
        generation,generation_notice,
    })
}

async fn touch(graph_id: &str) -> Result<(), ServerFnError> {
    sqlx::query("update graphs set updated_at = now() where id = $1::uuid")
        .bind(graph_id)
        .execute(pool()?)
        .await
        .map_err(db_error)?;
    Ok(())
}

/// Change a cell's name, prose or config (its type is fixed: a different
/// kind of cell is a new cell).
pub async fn update_cell(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_id: &str,
    form: CellForm<'_>,
) -> Result<Cell, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let old = cell_by_id(&ctx.row.graph.id, cell_id).await?;
    let cell_type = old.cell.cell_type;
    let (name, description, mut config) =
        validate_cell(cell_type, form.name, form.description, form.config).map_err(bad_request)?;
    let renamed = check_declaration(&ctx, &name, &description, &mut config, Some(&old.cell.id), cell_type).await?;
    check_cell_refs(&ctx, cell_type, &config).await?;
    let mut keep = old.raw_config.clone();
    // A renamed secret is another vault path: the old value goes.
    if cell_type == CellType::Secret && old.cell.config.name != config.name && old.cell.secret_set {
        delete_secret_value(&ctx, &old.cell).await?;
        if let Some(obj) = keep.as_object_mut() {
            obj.remove("set_at");
        }
    }
    let stored = stored_config(&config, Some(&keep));
    // What lode implements changed: the code is now older than the prose.
    let edited =
        name != old.cell.name || description != old.cell.description || config != old.cell.config;
    let mut transaction = super::connector::lock(&ctx.org.id).await?;
    super::connector::revoke_cell(&mut transaction, &ctx.org.id, &old.cell.id).await?;
    let updated = sqlx::query(
        "update graph_cells set name = $3, description = $4, config = $5::jsonb, \
         next_due_at = to_timestamp($6::bigint), updated_at = now(), \
         edited_at = case when $7 then now() else edited_at end \
         where graph_id = $1::uuid and id::text = $2",
    )
    .bind(&ctx.row.graph.id)
    .bind(&old.cell.id)
    .bind(&name)
    .bind(&description)
    .bind(stored.to_string())
    .bind(next_due(cell_type, &config))
    .bind(edited)
    .execute(&mut *transaction)
    .await;
    match updated {
        Ok(_) => {}
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            return Err(conflict(format!(
                "this notebook already has a cell '{name}'"
            )))
        }
        Err(e) => return Err(db_error(e)),
    }
    for dependent in renamed {
        sqlx::query("update graph_cells set description = $3, config = config || $4::jsonb, edited_at = now(), updated_at = now() where graph_id = $1::uuid and id = $2::uuid")
            .bind(&ctx.row.graph.id).bind(&dependent.id).bind(&dependent.description)
            .bind(serde_json::to_string(&dependent.config).map_err(|_| bad_request("invalid dependent declaration"))?)
            .execute(&mut *transaction).await.map_err(db_error)?;
    }
    transaction.commit().await.map_err(db_error)?;
    touch(&ctx.row.graph.id).await?;
    Ok(cell_by_id(&ctx.row.graph.id, &old.cell.id).await?.cell)
}

pub async fn delete_cell(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_id: &str,
) -> Result<(), ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let old = cell_by_id(&ctx.row.graph.id, cell_id).await?;
    let remaining: Vec<Cell> = cells_of(&ctx.row.graph.id).await?.into_iter().map(|row| row.cell).filter(|c| c.id != old.cell.id).collect();
    crate::validate_dependencies(&remaining).map_err(|_| conflict("remove this cell's named references from its dependents before deleting it"))?;
    if old.cell.cell_type == CellType::Secret && old.cell.secret_set {
        delete_secret_value(&ctx, &old.cell).await?;
    }
    let mut tx = super::connector::lock(&ctx.org.id).await?;
    super::connector::revoke_cell(&mut tx, &ctx.org.id, &old.cell.id).await?;
    sqlx::query("delete from graph_cells where id = $1::uuid")
        .bind(&old.cell.id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    touch(&ctx.row.graph.id).await?;
    Ok(())
}

/// Move a cell one place up (`-1`) or down (`1`).
pub async fn move_cell(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_id: &str,
    delta: i32,
) -> Result<(), ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let cells = cells_of(&ctx.row.graph.id).await?;
    let Some(i) = cells.iter().position(|c| c.cell.id == cell_id.trim()) else {
        return Err(not_found("no such cell"));
    };
    let j = i as i64 + delta.signum() as i64;
    if j < 0 || j as usize >= cells.len() {
        return Ok(());
    }
    // Renumber in the new order, so equal or sparse positions heal too.
    let mut order: Vec<String> = cells.iter().map(|c| c.cell.id.clone()).collect();
    order.swap(i, j as usize);
    let mut tx = pool()?.begin().await.map_err(db_error)?;
    for (position, id) in order.iter().enumerate() {
        sqlx::query("update graph_cells set position = $2 where id = $1::uuid")
            .bind(id)
            .bind(position as i32)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(())
}

// ── Secrets (§3.4) ──────────────────────────────────────────────────────

/// The vault path (under `secret/data/`) of a graph secret: the app may
/// create and delete there, and only lun reads.
pub fn secret_path(org_id: &str, graph_id: &str, name: &str) -> String {
    format!("graph/{org_id}/{graph_id}/{name}")
}

/// Delete the vault values of every secret cell of a project's notebooks,
/// before the project goes (its rows cascade; the vault does not).
pub async fn delete_project_secrets(org_id: &str, project_id: &str) -> Result<(), ServerFnError> {
    let mut tx = super::connector::lock(org_id).await?;
    let graphs = sqlx::query("select id::text as id from graphs where project_id=$1::uuid")
        .bind(project_id).fetch_all(&mut *tx).await.map_err(db_error)?;
    for graph in graphs { super::connector::revoke_graph(&mut tx, org_id, &graph.get::<String,_>("id")).await?; }
    tx.commit().await.map_err(db_error)?;
    let rows = sqlx::query(
        "select g.id::text as graph_id, c.config ->> 'name' as name from graph_cells c \
         join graphs g on g.id = c.graph_id \
         where g.project_id = $1::uuid and c.variant = 'secret' and c.config ? 'set_at'",
    )
    .bind(project_id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    for r in rows {
        let (graph_id, name): (String, Option<String>) = (r.get("graph_id"), r.get("name"));
        let Some(name) = name else { continue };
        vault::delete(&secret_path(org_id, &graph_id, &name))
            .await
            .map_err(|e| {
                eprintln!("vault delete for secret {name} of graph {graph_id} failed: {e}");
                bad_gateway("could not delete the project's secrets from the vault")
            })?;
    }
    Ok(())
}

async fn delete_secret_value(ctx: &Ctx, cell: &Cell) -> Result<(), ServerFnError> {
    let Some(name) = &cell.config.name else {
        return Ok(());
    };
    vault::delete(&secret_path(&ctx.org.id, &ctx.row.graph.id, name))
        .await
        .map_err(|e| {
            eprintln!(
                "vault delete for secret {name} of graph {} failed: {e}",
                ctx.row.graph.id
            );
            bad_gateway("could not delete the secret from the vault")
        })
}

/// Set a secret cell's value: into the vault, write-only. The app records
/// that it is set, never the value — there is no "show".
pub async fn set_secret(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_id: &str,
    value: &str,
) -> Result<Cell, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let cell = cell_by_id(&ctx.row.graph.id, cell_id).await?;
    if cell.cell.cell_type != CellType::Secret {
        return Err(bad_request("not a secret cell"));
    }
    let value = validate_secret_value(value).map_err(bad_request)?;
    if !vault::configured() {
        return Err(unavailable(
            "secrets are disabled: the vault is not configured",
        ));
    }
    let name = cell
        .cell
        .config
        .name
        .clone()
        .ok_or_else(|| bad_request("the secret has no name"))?;
    vault::write(
        &secret_path(&org.id, &ctx.row.graph.id, &name),
        &json!({ "kind": "secret", "value": value }),
    )
    .await
    .map_err(|e| bad_gateway(format!("could not store the secret in the vault: {e}")))?;
    sqlx::query(
        "update graph_cells set config = config || jsonb_build_object('set_at', now()::text), \
         updated_at = now() where id = $1::uuid",
    )
    .bind(&cell.cell.id)
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    Ok(cell_by_id(&ctx.row.graph.id, &cell.cell.id).await?.cell)
}

// ── Endpoints (§3.5) ────────────────────────────────────────────────────

/// A new token for an endpoint cell: the old URL stops working, the new
/// one is shown this once.
pub async fn rotate_endpoint(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_id: &str,
    public_url: &str,
) -> Result<CellSaved, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let cell = cell_by_id(&ctx.row.graph.id, cell_id).await?;
    if cell.cell.cell_type != CellType::Endpoint {
        return Err(bad_request("not an endpoint cell"));
    }
    let (url, hash) = endpoint_token(public_url)?;
    sqlx::query(
        "update graph_cells set config = config || jsonb_build_object('token_hash', $2::text), \
         updated_at = now() where id = $1::uuid",
    )
    .bind(&cell.cell.id)
    .bind(&hash)
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    Ok(CellSaved {
        cell: cell_by_id(&ctx.row.graph.id, &cell.cell.id).await?.cell,
        endpoint_url: Some(url),
        generation:None,generation_notice:None,
    })
}

/// A lun-style node, for answers to the world (endpoints).
fn node_json(n: &GraphNode) -> Value {
    let mut v = json!({ "id": n.id });
    if let Some(i) = &n.input {
        v["input"] = json!(i);
    }
    if let Some(f) = &n.function {
        v["function"] = json!(f);
        v["args"] = json!(n.args);
    }
    if let Some(o) = &n.outcome {
        if let Some(out) = &o.output {
            v["output"] = out.clone();
        } else if let Some(e) = &o.error {
            v["error"] = json!(e);
        } else if let Some(s) = o.skipped {
            v["skipped"] = json!(s);
        }
    }
    v
}

async fn record_endpoint_call(cell_id: &str, ok: bool, status: u16, fed: bool) {
    let Ok(pool) = pool() else { return };
    if let Err(e) = sqlx::query(
        "insert into endpoint_calls (cell_id, ok, status, fed_at) \
         values ($1::uuid, $2, $3, case when $4 then now() end)",
    )
    .bind(cell_id)
    .bind(ok)
    .bind(status as i32)
    .bind(fed)
    .execute(pool)
    .await
    {
        eprintln!("could not audit an endpoint call of cell {cell_id}: {e}");
    }
}

/// `POST /hooks/graphs/{token}`: the body feeds the cell's input, and the
/// answer is what changed. The token is looked up by its hash; every
/// delivery is audited in `endpoint_calls`.
pub async fn endpoint_delivery(token: &str, body: &[u8]) -> (u16, Value) {
    let hash = hex::encode(Sha256::digest(token.as_bytes()));
    let Ok(pool) = pool() else {
        return (503, json!({ "error": "the database is unavailable" }));
    };
    let row = match sqlx::query(
        "select c.id::text as id, c.graph_id::text as graph_id, c.config ->> 'input' as input, \
         c.impl ->> 'input_type' as input_type from graph_cells c \
         where c.variant = 'endpoint' and c.kind = 'source' and c.config ->> 'token_hash' = $1",
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return (404, json!({ "error": "no such endpoint" })),
        Err(e) => {
            eprintln!("endpoint lookup failed: {e}");
            return (503, json!({ "error": "the database is unavailable" }));
        }
    };
    let cell_id: String = row.get("id");
    let recent: i64 = sqlx::query(
        "select count(*) as n from endpoint_calls where cell_id = $1::uuid \
         and at > now() - interval '1 minute'",
    )
    .bind(&cell_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get("n"))
    .unwrap_or(0);
    if recent >= config::endpoint_calls_per_minute() {
        record_endpoint_call(&cell_id, false, 429, false).await;
        return (429, json!({ "error": "too many deliveries; slow down" }));
    }
    let value: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            record_endpoint_call(&cell_id, false, 400, false).await;
            return (
                400,
                json!({ "error": format!("the body is not JSON: {e}") }),
            );
        }
    };
    let input_type: Option<String> = row.get("input_type");
    if let Err(e) = json_fits(&value, input_type.as_deref()) {
        record_endpoint_call(&cell_id, false, 400, false).await;
        return (400, json!({ "error": e }));
    }
    let graph_id: String = row.get("graph_id");
    let input: Option<String> = row.get("input");
    let result = match (background_ctx(&graph_id).await, input) {
        (Ok(ctx), Some(input)) => feed(ctx, &input, value, "endpoint", Some(&cell_id)).await,
        (Err(e), _) => Err(e),
        (_, None) => Err("the endpoint has no input".to_string()),
    };
    match result {
        Ok(r) => {
            record_endpoint_call(&cell_id, true, 200, true).await;
            (
                200,
                json!({ "changed": r.changed.iter().map(node_json).collect::<Vec<_>>() }),
            )
        }
        Err(e) => {
            record_endpoint_call(&cell_id, false, 409, false).await;
            (409, json!({ "error": e }))
        }
    }
}

// ── Warrants for lode and lun (§5) ──────────────────────────────────────

/// The project's repository connection, and its owner.
async fn repo_connection(ctx: &Ctx) -> Result<(Connection, String, crate::RepoRef), ServerFnError> {
    let repo = ctx.project.repo.clone().ok_or_else(|| {
        bad_request(
            "set the project's primary repository first: the notebook's code is written there",
        )
    })?;
    let id = repo.connection_id.clone().ok_or_else(|| {
        bad_request("the connection the primary repository was read through was removed")
    })?;
    let (connection, owner) = connections::get(&ctx.org, &ctx.user, &id).await?;
    if repo.full_name.split('/').count() != 2 {
        return Err(bad_request("nested GitLab namespaces require a native repository-selector adapter"));
    }
    Ok((connection, owner, repo))
}

/// lode's three warrants (§2, step 1): repo `write`, the model with its
/// per-call cost, and repo `read` for lun.
async fn lode_credentials(ctx: &Ctx) -> Result<(Value, Connection), ServerFnError> {
    let (repo, repo_owner, source) = repo_connection(ctx).await?;
    let model_id = ctx
        .row
        .graph
        .model_connection_id
        .clone()
        .ok_or_else(|| bad_request("choose the AI connection the code is written with"))?;
    let (model, model_owner) = connections::get(&ctx.org, &ctx.user, &model_id).await?;
    if !super::db::org_settings(&ctx.org).await?.effect_policy.allows_provider(model.provider) {
        return Err(forbidden("this AI provider is not allowed by the organization"));
    }
    let name = ctx.row.graph.model_name.as_deref().or_else(|| default_model(model.provider)).unwrap_or("");
    if !model.provider.can_generate() || model.provider.ai_api(name) == Some(crate::AiApi::Classifier) {
        return Err(bad_request("choose a generative model before implementing this notebook"));
    }
    let cost = config::model_call_cost();
    let repo_root: Vec<String> = source.full_name.split('/').map(str::to_string).collect();
    let mut write_root = repo_root.clone();
    write_root.extend(project_path(&ctx.row.graph.slug).split('/').map(str::to_string));
    let mut write_permissions = super::connector::scoped(&repo, "repositories.write", write_root.clone(), true);
    // Publication plans may remove files only when independently requested by
    // the connection/admin ceiling. The read/write preset never adds deletion.
    let delete_permissions = super::connector::scoped(&repo, "repositories.delete", write_root, true);
    write_permissions.scopes.extend(delete_permissions.scopes);
    let repo_read = super::connector::mint(&ctx.org, &ctx.user, &repo, &repo_owner, "repositories.read",
        &super::connector::scoped(&repo, "repositories.read", repo_root.clone(), true), 0).await?.grant;
    let repo_write = super::connector::mint(&ctx.org, &ctx.user, &repo, &repo_owner, "repositories.write",
        &write_permissions, 0).await?.grant;
    let model_grant = super::connector::mint(&ctx.org, &ctx.user, &model, &model_owner, "inference.generate",
        &super::connector::scoped(&model, "inference.generate", crate::model_resource(model.provider, name), false), cost).await?.grant;
    let lun_read = super::connector::mint(&ctx.org, &ctx.user, &repo, &repo_owner, "repositories.read",
        &super::connector::scoped(&repo, "repositories.read", repo_root, true), 0).await?.grant;
    let mut repo_credentials = warrant::credentials_json(&repo_read, &repo_owner, &repo.id, None);
    repo_credentials["operations"] = json!([{ "operation": "repositories.write", "warrant": repo_write.warrant.to_json() }]);
    Ok((
        json!({
            "repo": repo_credentials,
            "model": warrant::credentials_json(&model_grant, &model_owner, &model.id, Some(cost)),
            "lun": warrant::credentials_json(&lun_read, &repo_owner, &repo.id, None),
        }),
        model,
    ))
}

/// Where a notebook's Lean project lives in the repository.
pub fn project_path(graph_slug: &str) -> String {
    format!("typednotes/{graph_slug}")
}

// ── Implement (§2, step 1) ──────────────────────────────────────────────

/// Serialized durable launches. Re-read actor/membership and all live ceilings;
/// a newer revision remains queued, and failures leave the saved cells intact.
async fn run_queued(graph_id:&str)->Result<bool,ServerFnError>{
    let mut tx=pool()?.begin().await.map_err(db_error)?;
    let locked=sqlx::query("select pg_try_advisory_xact_lock(hashtextextended($1,3)) as locked")
        .bind(graph_id).fetch_one(&mut *tx).await.map_err(db_error)?.get::<bool,_>("locked");
    if !locked{return Ok(true);}
    let request=sqlx::query("select user_id::text as actor,revision::text as revision from graph_generation_requests where graph_id=$1::uuid")
        .bind(graph_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let Some(request)=request else{return Ok(false);};
    let actor:String=request.get("actor");let revision:String=request.get("revision");
    let row=graph_by_id(graph_id).await?.ok_or_else(||not_found("notebook was removed"))?;
    let parent=sqlx::query("select org_id::text as org from projects where id=$1::uuid").bind(&row.project_id)
        .fetch_one(pool()?).await.map_err(db_error)?.get::<String,_>("org");
    let user=sqlx::query("select email::text as email,display_name from users where id=$1::uuid and deleted_at is null")
        .bind(&actor).fetch_optional(pool()?).await.map_err(db_error)?;
    let result=async{
        let user=user.ok_or_else(||forbidden("generation actor was removed"))?;
        let user=User{id:actor,email:user.get("email"),display_name:user.get("display_name")};
        let org=sqlx::query("select slug::text as slug from orgs where id=$1::uuid").bind(parent).fetch_one(pool()?).await.map_err(db_error)?.get::<String,_>("slug");
        let org=super::db::org_for_member(&org,&user.id).await?.ok_or_else(||forbidden("generation actor no longer belongs to this organization"))?;
        let(project,_)=super::projects::get(&org,&sqlx::query("select slug::text as slug from projects where id=$1::uuid").bind(&row.project_id).fetch_one(pool()?).await.map_err(db_error)?.get::<String,_>("slug")).await?;
        let mut ctx=Ctx{org,project,row,user};
        retire_writer(&mut ctx).await?;
        let cells=cells_of(graph_id).await?.into_iter().map(|r|r.cell).collect::<Vec<_>>();
        let text=lode_message(&ctx.row.graph.name,&cells,None);
        launch(&ctx,&text,Writing::All,LaunchedBy::Member,"Writing notebook code").await
    }.await;
    let removed=sqlx::query("delete from graph_generation_requests where graph_id=$1::uuid and revision=$2::uuid")
        .bind(graph_id).bind(revision).execute(&mut *tx).await.map_err(db_error)?.rows_affected();
    if let Err(e)=result {if removed!=0{fail_writing(graph_id,&format!("Automatic code generation could not start: {}",err_text(&e))).await?;}}
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub async fn tick_queued(){
    let Ok(pool)=pool() else{return;};
    let Ok(rows)=sqlx::query("select graph_id::text as id from graph_generation_requests order by requested_at limit 8").fetch_all(pool).await else{return;};
    for row in rows{let _=run_queued(&row.get::<String,_>("id")).await;}
}

/// New declarations need a fresh proof-bounded service ceiling. Never extend
/// an existing session's permitted function names or connector grants in place.
async fn retire_writer(ctx:&mut Ctx)->Result<(),ServerFnError>{
    if let Some(id)=ctx.row.lode_session_id.clone(){
        lode::narrow(&id,&[]).await.map_err(bad_gateway)?;
        lode::abort(&id).await.map_err(bad_gateway)?;
        let mut tx=super::connector::lock(&ctx.org.id).await?;
        super::connector::require_actor(&mut tx,&ctx.org.id,&ctx.user.id).await?;
        super::connector::revoke_graph(&mut tx,&ctx.org.id,&ctx.row.graph.id).await?;
        sqlx::query("update graphs set lode_session_id=null where id=$1::uuid").bind(&ctx.row.graph.id).execute(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        ctx.row.lode_session_id=None;
    }
    Ok(())
}

/// Which cells a lode run writes.
pub enum Writing<'a> {
    /// Every cell (the notebook's implementation).
    All,
    /// These cells (a rewrite), each with why.
    Cells(&'a [(String, String)]),
    /// Whichever were already being written (steering a run).
    Unchanged,
}

/// Who launched a run: a member (which resets the notebook's budget of
/// automatic rewrites), or the app itself (which spends it).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LaunchedBy {
    Member,
    App,
}

/// Send `text` to lode — a message to the notebook's session, which starts
/// a run or steers the one going, or a new session if lode has none — with
/// fresh warrants, and mark the cells it writes.
async fn launch(
    ctx: &Ctx,
    text: &str,
    writing: Writing<'_>,
    by: LaunchedBy,
    detail: &str,
) -> Result<Graph, ServerFnError> {
    if !lode::configured() {
        return Err(unavailable(
            "notebooks cannot be implemented: code writing is not configured",
        ));
    }
    let mut current=ctx.clone();
    if !matches!(writing,Writing::Unchanged){retire_writer(&mut current).await?;}
    let ctx=&current;
    let (credentials, model) = lode_credentials(ctx).await?;
    let mut policy = super::db::org_settings(&ctx.org).await?.effect_policy.validate().map_err(bad_request)?;
    let cells = cells_of(&ctx.row.graph.id).await?;
    let mut execution = writer_execution(ctx, &cells).await.map_err(bad_gateway)?;
    let mut writer_text = text.to_string();
    if cells.iter().any(|row| row.cell.cell_type == CellType::DbSink || row.cell.config.connectors.iter().any(|grant| grant.connection == "compute")) {
        if !policy.effects.iter().any(|effect| effect == "PostgreSQL") { return Err(forbidden("organization PostgreSQL effect denied")); }
        let schema = compute::ensure(&ctx.org.id, &ctx.org.slug, &ctx.user.id).await.map_err(bad_gateway)?;
        let target = compute::public_target(&schema).map_err(bad_gateway)?;
        writer_text.push_str(&format!("\nTrusted non-secret compute target: {target}. Use a statically matching PostgreSQL capability, schema-qualified structured query AST and only the declared tables/operations. The runtime resolves the password; never embed or request it.\n"));
    }
    for row in cells.iter().filter(|row| row.cell.cell_type == CellType::StorageSink) {
        let id = row.cell.config.connection_id.as_deref().ok_or_else(|| bad_request("storage cell has no connection"))?;
        let (connection, _) = connections::get(&ctx.org, &ctx.user, id).await?;
        let instruction = if connection.provider == Provider::Dropbox {
            "Use the scoped Connector effect with files.create and a UTF-8 contents payload; Dropbox is not an ObjectStore bucket."
        } else { "Use the scoped ObjectStore effect; putString options must be empty (no content-type/metadata overrides)." };
        writer_text.push_str(&format!("\nTrusted non-secret storage target: {}. {instruction}\n", json!({
            "cell": row.cell.name, "provider": connection.provider.id(), "connection": connection.id,
            "bucket": super::connector::bucket(&connection), "resource": row.cell.config.path.as_deref().unwrap_or("").trim_start_matches('/').split('/').collect::<Vec<_>>() })));
    }
    let running = ctx.row.graph.status == "implementing";
    // Where this run starts in lode's log: its length now (unless a run is
    // going, which this message joins).
    let mut log_start = ctx.row.graph.log_start as i64;
    let sent = match &ctx.row.lode_session_id {
        Some(id) => {
            if !running {
                if let lode::Reply::Ok(status) = lode::status(id).await.map_err(bad_gateway)? {
                    log_start = status.get("entries").and_then(Value::as_i64).unwrap_or(0);
                }
            }
            let status = match lode::status(id).await.map_err(bad_gateway)? {
                lode::Reply::Ok(status) => Some(status), lode::Reply::Gone => None,
            };
            if let Some(status) = &status {
                let tools = lode::narrowing_tools(status, &policy.tools).map_err(bad_gateway)?;
                let binding = super::connector::bind_writer(&ctx.org, &ctx.row.graph.id, id, &credentials, &tools).await?;
                policy.tools.retain(|tool| binding.policy.tools.contains(tool));
                execution = lode::bind_execution(&execution, &binding).map_err(bad_gateway)?;
            }
            lode::message(id, &writer_text, credentials.clone(), &policy.tools, &execution)
                .await
                .map_err(bad_gateway)?
        }
        None => lode::Reply::Gone,
    };
    let session_id = match sent {
        lode::Reply::Ok(()) => ctx.row.lode_session_id.clone().unwrap_or_default(),
        lode::Reply::Gone => {
            if matches!(writing, Writing::Unchanged) {
                return Err(conflict(
                    "the writing was interrupted (a restart); implement again",
                ));
            }
            let (_, _, repo) = repo_connection(ctx).await?;
            let body = json!({
                "source": {
                    "url": repo.web_url,
                    "branch": repo.default_branch.clone().unwrap_or_else(|| "main".to_string()),
                    "path": project_path(&ctx.row.graph.slug),
                    "credentials": credentials["repo"],
                },
                "model": {
                    "name": ctx.row.graph.model_name.clone()
                        .or_else(|| default_model(model.provider).map(str::to_string))
                        .unwrap_or_default(),
                    "api": model.provider.ai_api(ctx.row.graph.model_name.as_deref().or_else(|| default_model(model.provider)).unwrap_or("")).map(|api| api.id()).unwrap_or("openai"),
                    "baseUrl": model.base_url,
                    "credentials": credentials["model"],
                },
                "lun": { "credentials": credentials["lun"] },
                "agent": "build",
                "tools": policy.tools,
                "execution": execution,
                "buildContracts": build_contracts(&cells.iter().map(|r|r.cell.clone()).collect::<Vec<_>>()),
            });
            log_start = 0;
            // Opening the checkout precedes generation: the broker policy must
            // bind the actual persisted Lode session ID before a model call.
            let id = lode::open(&body).await.map_err(bad_gateway)?;
            let binding = super::connector::bind_writer(&ctx.org, &ctx.row.graph.id, &id, &credentials, &policy.tools).await?;
            policy.tools.retain(|tool| binding.policy.tools.contains(tool));
            execution = lode::bind_execution(&execution, &binding).map_err(bad_gateway)?;
            if !matches!(lode::message(&id, &writer_text, credentials.clone(), &policy.tools, &execution).await.map_err(bad_gateway)?, lode::Reply::Ok(())) {
                return Err(bad_gateway("writer session disappeared before starting"));
            }
            id
        }
    };
    let mut tx = pool()?.begin().await.map_err(db_error)?;
    sqlx::query(
        "update graphs set status = 'implementing', status_detail = $2, lode_session_id = $3, \
         lode_log_start = $4, auto_repairs = case when $5 then 0 else auto_repairs + 1 end, \
         updated_at = now() where id = $1::uuid",
    )
    .bind(&ctx.row.graph.id)
    .bind(detail)
    .bind(&session_id)
    .bind(log_start as i32)
    .bind(by == LaunchedBy::Member)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    match writing {
        Writing::All => {
            sqlx::query(
                "update graph_cells set writing = true, issue = null where graph_id = $1::uuid",
            )
            .bind(&ctx.row.graph.id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        }
        Writing::Cells(cells) => {
            for (id, issue) in cells {
                sqlx::query(
                    "update graph_cells set writing = true, issue = $3 \
                     where graph_id = $1::uuid and id::text = $2",
                )
                .bind(&ctx.row.graph.id)
                .bind(id)
                .bind(issue)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
            }
        }
        Writing::Unchanged => {}
    }
    tx.commit().await.map_err(db_error)?;
    Ok(graph_by_id(&ctx.row.graph.id)
        .await?
        .ok_or_else(|| not_found("no such notebook"))?
        .graph)
}

/// Ask lode to implement the notebook — or, while it runs and `steer` is
/// set, steer it with `note`.
pub async fn implement(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    note: &str,
    steer: bool,
) -> Result<Graph, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let cells: Vec<Cell> = cells_of(&ctx.row.graph.id)
        .await?
        .into_iter()
        .map(|c| c.cell)
        .collect();
    if cells.is_empty() {
        return Err(bad_request("add a cell first"));
    }
    let note = note.trim();
    if note.chars().count() > 4000 {
        return Err(bad_request("note: at most 4000 characters"));
    }
    if steer && ctx.row.graph.status == "implementing" {
        if note.is_empty() {
            return Err(bad_request("say how to steer"));
        }
        return launch(
            &ctx,
            note,
            Writing::Unchanged,
            LaunchedBy::Member,
            "writing the code (steered)",
        )
        .await;
    }
    let text = lode_message(&ctx.row.graph.name, &cells, Some(note));
    launch(
        &ctx,
        &text,
        Writing::All,
        LaunchedBy::Member,
        "writing the code of every cell",
    )
    .await
}

/// A member says a cell does not do the right thing: lode rewrites its code
/// (joining the run going, if any). The cell keeps running its current code
/// on lun until the rewrite is built.
pub async fn report_cell(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_id: &str,
    report: &str,
) -> Result<Graph, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let cell = cell_by_id(&ctx.row.graph.id, cell_id).await?.cell;
    let report = report.trim();
    if report.is_empty() || report.chars().count() > 4000 {
        return Err(bad_request("say what is wrong: 1–4000 characters"));
    }
    let text = repair_message(&ctx.row.graph.name, &cell, report);
    launch(
        &ctx,
        &text,
        Writing::Cells(&[(cell.id.clone(), report.to_string())]),
        LaunchedBy::Member,
        &format!("rewriting `{}`", cell.name),
    )
    .await
}

/// Whether the app may still send the notebook back to lode by itself:
/// within the org's cap (its settings page), else the deployment's.
async fn may_auto_repair(ctx: &Ctx) -> bool {
    i64::from(ctx.row.auto_repairs) < super::db::auto_repair_cap(&ctx.org.id).await
}

/// lun reported an error for the nodes of some cells: send each back to
/// lode — once per distinct error, and within the notebook's budget of
/// automatic rewrites. The notebook, now implementing, if a run launched.
async fn auto_repair_errors(
    ctx: &Ctx,
    cells: &[CellRow],
    changed: &[GraphNode],
    nodes: &[GraphNode],
) -> Option<Graph> {
    if ctx.row.graph.status == "implementing" || !may_auto_repair(ctx).await {
        return None;
    }
    let mut failing: Vec<(&CellRow, String)> = Vec::new();
    for c in cells
        .iter()
        .filter(|c| c.cell.cell_type.has_function() && !c.cell.writing)
    {
        let Some(function) = c
            .cell
            .implementation
            .as_ref()
            .and_then(|i| i.function.as_deref())
        else {
            continue;
        };
        let Some(node) = changed
            .iter()
            .find(|n| n.function.as_deref() == Some(function))
        else {
            continue;
        };
        let Some(error) = node.outcome.as_ref().and_then(|o| o.error.clone()) else {
            continue;
        };
        if c.auto_issue.as_deref() == Some(error.as_str()) {
            continue;
        }
        let args: Vec<String> = node
            .args
            .iter()
            .filter_map(|a| nodes.iter().find(|n| n.id == *a))
            .map(|n| {
                let value = n
                    .outcome
                    .as_ref()
                    .and_then(|o| o.output.as_ref())
                    .map(|v| v.to_string().chars().take(300).collect::<String>())
                    .unwrap_or_else(|| "no value".to_string());
                format!("{} = {value}", crate::node_label(n))
            })
            .collect();
        let issue = if args.is_empty() {
            format!("the code failed: {error}")
        } else {
            format!("the code failed: {error} (arguments: {})", args.join("; "))
        };
        failing.push((c, issue));
    }
    if failing.is_empty() {
        return None;
    }
    let text = failing
        .iter()
        .map(|(c, issue)| repair_message(&ctx.row.graph.name, &c.cell, issue))
        .collect::<Vec<_>>()
        .join("\n---\n\n");
    let marks: Vec<(String, String)> = failing
        .iter()
        .map(|(c, issue)| (c.cell.id.clone(), issue.clone()))
        .collect();
    let names: Vec<String> = failing
        .iter()
        .map(|(c, _)| format!("`{}`", c.cell.name))
        .collect();
    let launched = launch(
        ctx,
        &text,
        Writing::Cells(&marks),
        LaunchedBy::App,
        &format!("the code failed: rewriting {}", names.join(", ")),
    )
    .await;
    match launched {
        Ok(graph) => {
            if let Ok(pool) = pool() {
                for (c, _) in &failing {
                    let error = changed
                        .iter()
                        .find(|n| {
                            n.function.as_deref()
                                == c.cell
                                    .implementation
                                    .as_ref()
                                    .and_then(|i| i.function.as_deref())
                        })
                        .and_then(|n| n.outcome.as_ref())
                        .and_then(|o| o.error.clone());
                    let _ =
                        sqlx::query("update graph_cells set auto_issue = $2 where id = $1::uuid")
                            .bind(&c.cell.id)
                            .bind(error)
                            .execute(pool)
                            .await;
                }
            }
            Some(graph)
        }
        Err(e) => {
            eprintln!(
                "could not send notebook {} back to lode: {}",
                ctx.row.graph.id,
                err_text(&e)
            );
            None
        }
    }
}

/// The build of lode's commit failed: send it back to lode with lun's
/// diagnostics, within the budget of automatic rewrites. Whether it did.
async fn auto_repair_build(ctx: &Ctx, diagnostics: &str) -> bool {
    if !may_auto_repair(ctx).await {
        return false;
    }
    let Ok(cells) = cells_of(&ctx.row.graph.id).await else {
        return false;
    };
    // The cells the diagnostics name (`function total: …`), else all of them.
    let named: Vec<&CellRow> = cells
        .iter()
        .filter(|c| crate::mentions(diagnostics, &c.cell.name))
        .collect();
    let concerned: Vec<&CellRow> = if named.is_empty() {
        cells
            .iter()
            .filter(|c| c.cell.cell_type.has_function())
            .collect()
    } else {
        named
    };
    let refs: Vec<&Cell> = concerned.iter().map(|c| &c.cell).collect();
    let text = build_repair_message(&ctx.row.graph.name, &refs, diagnostics);
    let issue = format!(
        "the code did not build: {}",
        diagnostics.lines().next().unwrap_or("")
    );
    let marks: Vec<(String, String)> = concerned
        .iter()
        .map(|c| (c.cell.id.clone(), issue.clone()))
        .collect();
    match launch(
        ctx,
        &text,
        Writing::Cells(&marks),
        LaunchedBy::App,
        "the code did not build: fixing it",
    )
    .await
    {
        Ok(_) => true,
        Err(e) => {
            eprintln!(
                "could not send notebook {} back to lode: {}",
                ctx.row.graph.id,
                err_text(&e)
            );
            false
        }
    }
}

/// Stop lode's run.
pub async fn abort(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
) -> Result<Graph, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let mut guard=pool()?.begin().await.map_err(db_error)?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1,3))").bind(&ctx.row.graph.id).execute(&mut *guard).await.map_err(db_error)?;
    sqlx::query("delete from graph_generation_requests where graph_id=$1::uuid").bind(&ctx.row.graph.id).execute(&mut *guard).await.map_err(db_error)?;
    let ctx=member_ctx(org,user,project_slug,graph_slug).await?;
    if let Some(id) = &ctx.row.lode_session_id {
        lode::abort(id).await.map_err(bad_gateway)?;
    }
    fail_writing(&ctx.row.graph.id, "aborted").await?;
    guard.commit().await.map_err(db_error)?;
    Ok(graph_by_id(&ctx.row.graph.id)
        .await?
        .ok_or_else(|| not_found("no such notebook"))?
        .graph)
}

/// Follow lode's log from `after`, holding up to `wait` seconds for news,
/// and move the notebook on when the run is over: build what lode published,
/// then adopt the build once lun has it ready.
pub async fn progress(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    after: u64,
    wait: u64,
) -> Result<LodeProgress, ServerFnError> {
    let mut ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let queued=sqlx::query("select 1 from graph_generation_requests where graph_id=$1::uuid").bind(&ctx.row.graph.id)
        .fetch_optional(pool()?).await.map_err(db_error)?.is_some();
    if queued {
        let id=ctx.row.graph.id.clone();
        // Launch independently of request cancellation so navigation cannot lose
        // checkout progress. SQL serialization prevents duplicate paid runs.
        spawn_generation(id.clone());
        reload(&mut ctx).await?;
        return Ok(LodeProgress{graph:ctx.row.graph,state:"queued".into(),entries:Vec::new(),next:after});
    }
    let mut entries = Vec::new();
    let mut next = after;
    let mut state = "none".to_string();
    if let (Some(id), true) = (ctx.row.lode_session_id.clone(), lode::configured()) {
        let wait = if ctx.row.graph.status == "implementing" {
            wait
        } else {
            0
        };
        match lode::log(&id, after, wait).await.map_err(bad_gateway)? {
            lode::Reply::Gone => {
                sqlx::query("update graphs set lode_session_id = null where id = $1::uuid")
                    .bind(&ctx.row.graph.id)
                    .execute(pool()?)
                    .await
                    .map_err(db_error)?;
                if ctx.row.graph.status == "implementing" && ctx.row.graph.build_id.is_none() {
                    fail_writing(
                        &ctx.row.graph.id,
                        "the writing was interrupted (a restart): implement again",
                    )
                    .await?;
                }
            }
            lode::Reply::Ok((es, n, running)) => {
                entries = es;
                next = n;
                state = if running { "running" } else { "idle" }.to_string();
                if running {
                    refresh_warrants(&ctx, &id).await;
                }
            }
        }
    }
    if ctx.row.graph.status == "implementing" {
        if let Err(e) = advance(&mut ctx, state == "running").await {
            eprintln!(
                "notebook {} did not advance: {}",
                ctx.row.graph.id,
                err_text(&e)
            );
            fail_writing(&ctx.row.graph.id, &err_text(&e)).await?;
        }
        reload(&mut ctx).await?;
    }
    Ok(LodeProgress {
        graph: ctx.row.graph,
        state,
        entries,
        next,
    })
}

fn spawn_generation(graph:String){tokio::spawn(async move{let _=run_queued(&graph).await;});}

/// Fresh warrants for lode's run in flight: they live 300 s, a run lasts
/// longer (§5), and lode keeps them in memory only.
async fn refresh_warrants(ctx: &Ctx, lode_session: &str) {
    match lode_credentials(ctx).await {
        Ok((credentials, _)) => {
            let tools = match super::db::org_settings(&ctx.org).await {
                Ok(settings) => settings.effect_policy.tools,
                Err(_) => return,
            };
            let status = match lode::status(lode_session).await {
                Ok(lode::Reply::Ok(status)) => status, _ => return,
            };
            let tools = match lode::narrowing_tools(&status, &tools) { Ok(tools) => tools, Err(_) => return };
            let cells = match cells_of(&ctx.row.graph.id).await { Ok(cells) => cells, Err(_) => return };
            let execution = match writer_execution(ctx, &cells).await { Ok(execution) => execution, Err(_) => return };
            let binding = match super::connector::bind_writer(&ctx.org, &ctx.row.graph.id, lode_session, &credentials, &tools).await { Ok(binding) => binding, Err(_) => return };
            let tools: Vec<_> = tools.into_iter().filter(|tool| binding.policy.tools.contains(tool)).collect();
            let execution = match lode::bind_execution(&execution, &binding) { Ok(execution) => execution, Err(_) => return };
            if let Err(e) = lode::refresh(lode_session, credentials, &tools, &execution).await {
                eprintln!(
                    "could not refresh lode's warrants for {}: {e}",
                    ctx.row.graph.id
                );
            }
        }
        Err(e) => eprintln!(
            "could not mint lode's warrants for {}: {}",
            ctx.row.graph.id,
            err_text(&e)
        ),
    }
}

/// The scheduler's share of implementation: keep every running lode
/// session supplied with warrants, and move finished ones on (build, then
/// session) — so a notebook gets built whether or not its page is open.
pub async fn tick_implementing() {
    let Ok(pool) = pool() else { return };
    let ids: Vec<String> = match sqlx::query(
        "select id::text as id from graphs where status = 'implementing' \
         and updated_at > now() - interval '1 day'",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows.iter().map(|r| r.get("id")).collect(),
        Err(e) => {
            eprintln!("scheduler: implementing notebooks: {e}");
            return;
        }
    };
    for id in ids {
        let mut ctx = match background_ctx(&id).await {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("scheduler: notebook {id}: {e}");
                continue;
            }
        };
        let running = match &ctx.row.lode_session_id {
            Some(session) => match lode::status(session).await {
                Ok(lode::Reply::Ok(status)) => {
                    let running = lode::running(&status);
                    if running {
                        refresh_warrants(&ctx, session).await;
                    }
                    running
                }
                Ok(lode::Reply::Gone) => false,
                Err(e) => {
                    eprintln!("scheduler: notebook {id}: {e}");
                    continue;
                }
            },
            None => false,
        };
        if let Err(e) = advance(&mut ctx, running).await {
            let _ = fail_writing(&id, &err_text(&e)).await;
        }
    }
}

/// One step of the implementing state: wait for lode, then submit the
/// published commit to lun, then adopt the ready build.
async fn advance(ctx: &mut Ctx, lode_running: bool) -> Result<(), ServerFnError> {
    let mut guard=pool()?.begin().await.map_err(db_error)?;
    let locked=sqlx::query("select pg_try_advisory_xact_lock(hashtextextended($1,3)) as locked").bind(&ctx.row.graph.id).fetch_one(&mut *guard).await.map_err(db_error)?.get::<bool,_>("locked");
    if !locked{return Ok(());}
    reload(ctx).await?;
    if ctx.row.graph.status!="implementing"{return Ok(());}
    if sqlx::query("select 1 from graph_generation_requests where graph_id=$1::uuid").bind(&ctx.row.graph.id).fetch_optional(pool()?).await.map_err(db_error)?.is_some(){return Ok(());}
    if lode_running {
        return Ok(());
    }
    // lode's run is over: what did it publish?
    let status = match &ctx.row.lode_session_id {
        Some(id) => match lode::status(id).await.map_err(bad_gateway)? {
            lode::Reply::Ok(s) => Some(s),
            lode::Reply::Gone => None,
        },
        None => None,
    };
    if status.as_ref().is_some_and(lode::running) {
        return Ok(());
    }
    let head = match status.as_ref().and_then(lode::head) {
        Some(head) => head,
        None => match &ctx.row.graph.commit {
            Some(c) => c.clone(),
            None => branch_head(ctx).await?,
        },
    };
    if ctx.row.graph.commit.as_deref() != Some(head.as_str()) || ctx.row.graph.build_id.is_none() {
        let request = match build_request(ctx, &head).await {
            Ok((request, _)) => request,
            Err(e) => {
                let lode_error = status
                    .as_ref()
                    .and_then(|s| s.get("error"))
                    .and_then(Value::as_str)
                    .map(|e| format!(" (while writing the code: {e})"))
                    .unwrap_or_default();
                let why = format!("{}{lode_error}", err_text(&e));
                // No usable lun.json at lode's commit: another run, if the
                // budget allows (lode's own error is not the code's fault).
                if lode_error.is_empty() && auto_repair_build(ctx, &why).await {
                    return Ok(());
                }
                return Err(bad_request(why));
            }
        };
        let build = lun::submit(&request, GRAPH_NAME)
            .await
            .map_err(bad_gateway)?;
        sqlx::query(
            "update graphs set commit_sha = $2, lun_build_id = $3, status_detail = $4, \
             updated_at = now() where id = $1::uuid",
        )
        .bind(&ctx.row.graph.id)
        .bind(&head)
        .bind(&build.id)
        .bind(format!("building {}", &head[..head.len().min(12)]))
        .execute(pool()?)
        .await
        .map_err(db_error)?;
        reload(ctx).await?;
    }
    let Some(build_id) = ctx.row.graph.build_id.clone() else {
        return Ok(());
    };
    let build = match lun::build(&build_id, GRAPH_NAME)
        .await
        .map_err(bad_gateway)?
    {
        Some(b) => b,
        None => {
            // lun forgot it: submit it again next time.
            sqlx::query("update graphs set lun_build_id = null where id = $1::uuid")
                .bind(&ctx.row.graph.id)
                .execute(pool()?)
                .await
                .map_err(db_error)?;
            return Ok(());
        }
    };
    match build.state.as_str() {
        "ready" => adopt(ctx, &build).await,
        "failed" => {
            let mut why = build
                .error
                .clone()
                .unwrap_or_else(|| "the build failed".to_string());
            for d in build.diagnostics.iter().take(20) {
                why.push('\n');
                why.push_str(d);
            }
            if auto_repair_build(ctx, &why).await {
                return Ok(());
            }
            fail_writing(&ctx.row.graph.id, &why).await
        }
        other => {
            set_status(
                &ctx.row.graph.id,
                "implementing",
                Some(&format!("building ({other})")),
            )
            .await
        }
    }
}

/// A run that ended without a build: the notebook failed, its cells no
/// longer being written (they keep the code they had, if any).
async fn fail_writing(graph_id: &str, why: &str) -> Result<(), ServerFnError> {
    sqlx::query("update graph_cells set writing = false where graph_id = $1::uuid")
        .bind(graph_id)
        .execute(pool()?)
        .await
        .map_err(db_error)?;
    set_status(graph_id, "failed", Some(why)).await
}

/// Rebuild from what the repository holds now (after a restart, or a push
/// made outside lode).
pub async fn rebuild(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
) -> Result<Graph, ServerFnError> {
    if !lun::configured() {
        return Err(unavailable(
            "notebooks cannot be built: the code runtime is not configured",
        ));
    }
    let mut ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    if ctx.row.graph.status == "implementing" {
        return Err(conflict(
            "the notebook's code is being written; wait for it",
        ));
    }
    let head = branch_head(&ctx).await?;
    sqlx::query(
        "update graphs set status = 'implementing', status_detail = 'reading the repository', \
         commit_sha = null, lun_build_id = null, lode_session_id = null, updated_at = now() \
         where id = $1::uuid",
    )
    .bind(&ctx.row.graph.id)
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    reload(&mut ctx).await?;
    ctx.row.graph.commit = Some(head);
    if let Err(e) = advance(&mut ctx, false).await {
        fail_writing(&ctx.row.graph.id, &err_text(&e)).await?;
    }
    Ok(graph_by_id(&ctx.row.graph.id)
        .await?
        .ok_or_else(|| not_found("no such notebook"))?
        .graph)
}

// ── Build (§2, step 2) ──────────────────────────────────────────────────

/// Single trusted definition used for both writer trials and final adoption.
/// Generated signatures do not become user pins; only configuration does.
fn build_contracts(cells:&[Cell])->Value{
    let outputs:Map<String,Value>=cells.iter().filter(|c|c.cell_type.has_function())
        .filter_map(|c|c.config.output_type.as_ref().map(|t|(c.name.clone(),json!(t)))).collect();
    let inputs:Map<String,Value>=cells.iter().filter(|c|c.cell_type.has_input())
        .filter_map(|c|c.config.output_type.as_ref().map(|t|(c.config.input.clone().unwrap_or_else(||c.name.clone()),json!(t)))).collect();
    let dependencies:Map<String,Value>=cells.iter().filter(|c|c.cell_type.in_graph()).filter_map(|c|c.config.dependencies.as_ref().map(|deps|{
        let args=deps.iter().map(|name|cells.iter().find(|p|p.name==*name).filter(|p|p.cell_type.has_input()).and_then(|p|p.config.input.clone()).unwrap_or_else(||name.clone())).collect::<Vec<_>>();
        (c.name.clone(),json!(args))
    })).collect();
    json!({"outputs":outputs,"inputs":inputs,"dependencies":dependencies,"graph":GRAPH_NAME})
}

/// The head commit of the repository's branch, read through its connection.
async fn branch_head(ctx: &Ctx) -> Result<String, ServerFnError> {
    let (connection, owner, repo) = repo_connection(ctx).await?;
    let branch = repo
        .default_branch
        .clone()
        .unwrap_or_else(|| "main".to_string());
    let pointer = if connection.provider == Provider::Gitlab { "/commit/id" } else { "/commit/sha" };
    let body = connections::call_ok(
        &ctx.org,
        &ctx.user,
        &connection,
        &owner,
        ProviderCall::new("repositories.read", repo.full_name.split('/').map(str::to_string).collect(), json!({"view": "branch", "ref": branch})),
    )
    .await?;
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    json.pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| bad_gateway("the code host did not say which commit the branch is at"))
}

/// A file of the notebook's project at `commit`, read through the
/// repository's connection; `path` is relative to the project directory.
async fn read_repo_file(ctx: &Ctx, path: &str, commit: &str) -> Result<Vec<u8>, ServerFnError> {
    let (connection, owner, repo) = repo_connection(ctx).await?;
    let file = format!("{}/{path}", project_path(&ctx.row.graph.slug));
    let resource = repo.full_name.split('/').chain(file.split('/')).map(str::to_string).collect();
    let body = connections::call_ok(
        &ctx.org,
        &ctx.user,
        &connection,
        &owner,
        ProviderCall::new("repositories.read", resource, json!({"ref": commit})),
    )
    .await
    .map_err(|e| {
        bad_request(format!(
            "no {file} at {}: {}",
            &commit[..commit.len().min(12)],
            err_text(&e)
        ))
    })?;
    decode_repository_file(&body)
}

/// Native repository reads return immutable blob/file metadata with base64
/// content. No download_url is ever followed with or without credentials.
fn decode_repository_file(body: &[u8]) -> Result<Vec<u8>, ServerFnError> {
    use base64::Engine;
    let value: Value = serde_json::from_slice(body).map_err(|_| bad_gateway("unreadable native repository file"))?;
    if value.get("encoding").and_then(Value::as_str) != Some("base64") {
        return Err(bad_gateway("native repository file is not base64 encoded"));
    }
    let content = value.get("content").and_then(Value::as_str).ok_or_else(|| bad_gateway("native repository file has no content"))?;
    let content: String = content.chars().filter(|ch| *ch != '\n' && *ch != '\r').collect();
    base64::engine::general_purpose::STANDARD.decode(content).map_err(|_| bad_gateway("invalid base64 repository file"))
}

/// The notebook's `lun.json` at `commit`.
async fn read_lun_json(ctx: &Ctx, commit: &str) -> Result<LunJson, ServerFnError> {
    let body = read_repo_file(ctx, "lun.json", commit).await?;
    serde_json::from_slice(&body).map_err(|e| {
        bad_request(format!(
            "the notebook's manifest (lun.json) is unreadable: {e}"
        ))
    })
}

/// A cell's code: its function's definition in the published commit, and,
/// while lode writes it, the unpublished changes that mention it.
pub async fn cell_code(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_id: &str,
) -> Result<crate::CellCode, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let cell = cell_by_id(&ctx.row.graph.id, cell_id).await?.cell;
    let mut code = crate::CellCode::default();
    if let (Some(imp), Some(commit)) = (cell.implementation.as_ref(), ctx.row.graph.commit.as_ref())
    {
        if let Some(file) = imp.module.as_deref().and_then(crate::module_file) {
            if let Ok(source) = read_repo_file(&ctx, &file, commit).await {
                let source = String::from_utf8_lossy(&source);
                let constant = imp
                    .constant
                    .clone()
                    .or_else(|| imp.function.clone())
                    .unwrap_or_default();
                code.published = crate::extract_definition(&source, &constant)
                    .or_else(|| Some(source.lines().take(200).collect::<Vec<_>>().join("\n")));
            }
            code.path = Some(format!("{}/{file}", project_path(&ctx.row.graph.slug)));
        }
    }
    if cell.writing && ctx.row.graph.status == "implementing" {
        if let Some(id) = &ctx.row.lode_session_id {
            if let Ok(lode::Reply::Ok(diff)) = lode::diff(id).await {
                code.draft = crate::diff_hunks_mentioning(&diff, &cell.name);
            }
        }
    }
    Ok(code)
}

/// lun's build request for `commit`: the source (with lun's repo `read`
/// warrant) and what `lun.json` declares.
async fn build_request(ctx: &Ctx, commit: &str) -> Result<(Value, LunJson), ServerFnError> {
    let lun_json = read_lun_json(ctx, commit).await?;
    let (connection, owner, repo) = repo_connection(ctx).await?;
    let read = super::connector::mint(&ctx.org, &ctx.user, &connection, &owner, "repositories.read",
        &super::connector::scoped(&connection, "repositories.read", repo.full_name.split('/').map(str::to_string).collect(), true), 0).await?.grant;
    let cells: Vec<Cell> = cells_of(&ctx.row.graph.id).await?.into_iter().map(|row| row.cell).collect();
    let contracts=build_contracts(&cells);
    let mut functions = serde_json::to_value(&lun_json.functions).map_err(|_| bad_request("invalid function declarations"))?;
    if let Some(functions) = functions.as_array_mut() {
        for function in functions {
            if let Some(cell) = cells.iter().find(|cell| function.get("name").and_then(Value::as_str) == Some(&cell.name)) {
                if let Some(ty) = contracts["outputs"].get(&cell.name) { function["outputType"] = ty.clone(); }
            }
        }
    }
    let mut graphs = serde_json::to_value(&lun_json.graphs).map_err(|_| bad_request("invalid graph declarations"))?;
    if let Some(graphs) = graphs.as_array_mut() {
        for graph in graphs.iter_mut().filter(|g| g.get("name").and_then(Value::as_str) == Some(GRAPH_NAME)) {
            graph["dependencies"] = contracts["dependencies"].clone();
            graph["inputTypes"] = contracts["inputs"].clone();
        }
    }
    let request = json!({
        "source": {
            "url": repo.web_url,
            "branch": repo.default_branch.clone().unwrap_or_else(|| "main".to_string()),
            "commit": commit,
            "path": project_path(&ctx.row.graph.slug),
            "credentials": warrant::credentials_json(&read, &owner, &connection.id, None),
        },
        "open": lun_json.open,
        "functions": functions,
        "graphs": graphs,
    });
    Ok((request, lun_json))
}

/// A ready build: record each cell's implementation and the graph's
/// structure, then register a fresh session.
async fn adopt(ctx: &mut Ctx, build: &lun::Build) -> Result<(), ServerFnError> {
    let commit = ctx.row.graph.commit.clone().unwrap_or_default();
    let lun_json = read_lun_json(ctx, &commit).await?;
    let structure = build
        .graph
        .clone()
        .ok_or_else(|| bad_request(format!("the build has no graph named '{GRAPH_NAME}'")))?;
    let cells = cells_of(&ctx.row.graph.id).await?;
    let mut tx = pool()?.begin().await.map_err(db_error)?;
    for c in &cells {
        let imp = cell_impl(c.cell.cell_type, &c.cell.name, &c.cell.config, &lun_json);
        let has_code = !c.cell.cell_type.has_function() || imp.function.is_some();
        // The run is over for every cell: those with code now run it on lun.
        sqlx::query(
            "update graph_cells set impl = $2::jsonb, next_due_at = to_timestamp($3::bigint), \
             writing = false, issue = case when $4 then null else issue end, \
             code_at = case when $4 then now() else code_at end \
             where id = $1::uuid",
        )
        .bind(&c.cell.id)
        .bind(serde_json::to_string(&imp).unwrap_or_default())
        .bind(next_due(c.cell.cell_type, &c.cell.config))
        .bind(has_code)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    }
    let missing: Vec<&str> = cells
        .iter()
        .filter(|c| c.cell.cell_type.has_function())
        .filter(|c| !lun_json.functions.iter().any(|f| f.name == c.cell.name))
        .map(|c| c.cell.name.as_str())
        .collect();
    sqlx::query(
        "update graphs set status = 'ready', status_detail = $2, structure = $3::jsonb, \
         last_nodes = $4::jsonb, lun_session_id = null, session_build_id = $5, \
         updated_at = now() where id = $1::uuid",
    )
    .bind(&ctx.row.graph.id)
    .bind(if missing.is_empty() {
        None
    } else {
        Some(format!("no code was written for: {}", missing.join(", ")))
    })
    .bind(structure.to_json().to_string())
    .bind(serde_json::to_string(&structure.nodes).unwrap_or_else(|_| "[]".into()))
    .bind(&build.id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    if let Some(old) = &ctx.row.lun_session_id {
        lun::end(old).await;
    }
    reload(ctx).await?;
    match register(ctx).await {
        Ok(answer) => {
            // The code builds; if it fails on the recorded inputs, it goes
            // back to lode (within the budget).
            let cells = cells_of(&ctx.row.graph.id).await?;
            auto_repair_errors(ctx, &cells, &answer.changed, &answer.nodes).await;
        }
        Err(e) => {
            set_status(
                &ctx.row.graph.id,
                "ready",
                Some(&format!("built, but the session could not start: {e}")),
            )
            .await?
        }
    }
    Ok(())
}

// ── Run (§2, step 3) ────────────────────────────────────────────────────

/// The warrants a session's `storage` sinks write with (§4.3), fresh for
/// every update.
async fn storage_warrants(ctx: &Ctx, cells: &[CellRow]) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for c in cells
        .iter()
        .filter(|c| c.cell.cell_type == CellType::StorageSink)
    {
        let Some(id) = &c.cell.config.connection_id else {
            continue;
        };
        let (connection, owner) = connections::get(&ctx.org, &ctx.user, id)
            .await
            .map_err(|e| format!("storage sink {}: {}", c.cell.name, err_text(&e)))?;
        let cost = config::storage_write_cost();
        let operation = match connection.provider {
            Provider::S3 | Provider::Azure => "objects.write",
            Provider::Dropbox => "files.create",
            _ => return Err("this storage sink has no native contents-upload adapter".into()),
        };
        let path = c.cell.config.path.as_deref().ok_or("storage sink has no path")?;
        let mut requested = super::connector::scoped(&connection, operation, path.trim_start_matches('/').split('/').map(str::to_string).collect(), false);
        if let Some(permissions) = c.cell.config.connectors.iter().find(|grant| grant.connection == connection.id).and_then(|grant| grant.permissions.as_ref()) {
            requested = requested.intersect(permissions);
        }
        let grant = super::connector::mint_for_cell(&ctx.org, &ctx.user, &connection, &owner, operation, &requested, cost, Some(&c.cell.id)).await
            .map_err(|e| err_text(&e))?.grant;
        out.push(json!({
            "cell": c.cell.name,
            "provider": connection.provider.id(),
            "connection_id": connection.id,
            "path": c.cell.config.path,
            "credentials": warrant::credentials_json(&grant, &owner, &connection.id, Some(cost)),
        }));
    }
    Ok(out)
}

/// Fresh operation-specific warrants and all independently owned ceilings.
/// This metadata is assembled from authenticated rows, never from `lun.json`.
async fn connector_grants(ctx: &Ctx, cells: &[CellRow]) -> Result<Value, String> {
    let text = |e: ServerFnError| err_text(&e);
    let policy = super::db::org_settings(&ctx.org).await.map_err(text)?.effect_policy;
    let mut functions = Map::new();
    for cell in cells.iter().filter(|row| row.cell.cell_type.has_function()) {
        let mut grants = Vec::new();
        let mut selected_connections = cell.cell.config.connectors.clone();
        if cell.cell.cell_type == CellType::StorageSink {
            if let Some(id) = &cell.cell.config.connection_id {
                if !selected_connections.iter().any(|grant| grant.connection == *id) {
                    selected_connections.push(crate::CellConnector { connection: id.clone(), permissions: None });
                }
            }
        }
        for selected in &selected_connections {
            if crate::LocalService::from_selection(&selected.connection).is_some() { continue; }
            let (connection, owner) = connections::get(&ctx.org, &ctx.user, &selected.connection).await.map_err(text)?;
            let object_store = matches!(connection.provider, Provider::S3 | Provider::Azure)
                && policy.effects.iter().any(|effect| effect == "ObjectStore");
            if (!object_store && !policy.effects.iter().any(|effect| effect == "Connector")) || !policy.allows_provider(connection.provider) {
                return Err(format!("organization permission denied: {}", connection.provider.name()));
            }
            let mut parent = connection.permissions.clone().unwrap_or_else(|| crate::ConnectorPermissions::preset(connection.provider, crate::PermissionPreset::ReadOnly));
            let mut organization = policy.connector_ceilings.get(connection.provider.id()).cloned().unwrap_or_else(|| parent.clone());
            let mut requested = selected.permissions.clone().unwrap_or_else(|| parent.clone());
            if cell.cell.cell_type == CellType::StorageSink && cell.cell.config.connection_id.as_ref() == Some(&selected.connection) {
                let operation = match connection.provider {
                    Provider::S3 | Provider::Azure => "objects.write",
                    Provider::Dropbox => "files.create",
                    _ => return Err("this storage sink has no native contents-upload adapter".into()),
                };
                let path = cell.cell.config.path.as_deref().ok_or("storage sink has no path")?;
                requested = requested.intersect(&super::connector::scoped(&connection, operation,
                    path.trim_start_matches('/').split('/').map(str::to_string).collect(), false));
            }
            requested.validate(connection.provider)?;
            parent.narrow(&requested)?;
            let mut effective = requested.intersect(&organization);
            let operations: std::collections::BTreeSet<_> = effective.scopes.iter().map(|scope| scope.operation.clone()).collect();
            let mut warrants = Vec::new();
            for operation in operations {
                let cost = if operation == "inference.generate" || operation == "classification.evaluate" { config::model_call_cost() } else { 0 };
                let minted = super::connector::mint_for_cell(&ctx.org, &ctx.user, &connection, &owner, &operation, &effective, cost, Some(&cell.cell.id)).await.map_err(text)?;
                parent = parent.intersect(&minted.connection);
                organization = organization.intersect(&minted.organization);
                effective = effective.intersect(&minted.cell);
                let grant = minted.grant;
                warrants.push(json!({"operation": operation, "warrant": grant.warrant.to_json(), "cost": cost}));
            }
            grants.push(json!({
                "provider": connection.provider.id(), "connection": connection.id,
                "account": format!("{owner}/{}", connection.id),
                "bucket": super::connector::bucket(&connection),
                "organization": organization.capability_json(connection.provider, &connection.id),
                "connectionPermissions": parent.capability_json(connection.provider, &connection.id),
                "cell": effective.capability_json(connection.provider, &connection.id),
                "warrants": warrants,
            }));
        }
        for service in [crate::LocalService::Postgres, crate::LocalService::Vault] {
            let explicit = cell.cell.config.connectors.iter().any(|grant| grant.connection == service.selection());
            let secrets: Vec<String> = cells.iter().filter(|row| row.cell.cell_type == CellType::Secret && row.cell.secret_set)
                .filter_map(|row| row.cell.config.name.clone()).collect();
            let needed = explicit || (service == crate::LocalService::Postgres && cell.cell.cell_type == CellType::DbSink)
                || (service == crate::LocalService::Vault && !secrets.is_empty());
            if !needed || !policy.effects.iter().any(|effect| effect == service.effect()) { continue; }
            let schema = if service == crate::LocalService::Postgres {
                if cell.cell.cell_type == CellType::DbSink {
                    compute::ensure_sink_table(&ctx.org.id, &ctx.org.slug, &ctx.user.id,
                        cell.cell.config.table.as_deref().ok_or("database sink has no table")?).await?
                } else { compute::ensure(&ctx.org.id, &ctx.org.slug, &ctx.user.id).await? }
            } else { String::new() };
            let requested = super::local::declared(service, &cell.cell.config, cell.cell.cell_type, &schema, &secrets)?;
            grants.push(super::local::mint(&ctx.org, &ctx.user, &cell.cell, service, &requested).await.map_err(text)?);
        }
        functions.insert(cell.cell.name.clone(), json!(grants));
    }
    Ok(json!(functions))
}

/// The writer gets the same freshly provisioned function grants as graph/source
/// execution. Only the actor comes from Ctx; external accounts retain their real
/// credential owners. Transport and private runtime configuration remain Lun's.
async fn writer_execution(ctx: &Ctx, cells: &[CellRow]) -> Result<Value, String> {
    let policy = super::db::org_settings(&ctx.org).await.map_err(|e| err_text(&e))?
        .effect_policy.validate()?.for_cells(&cells.iter().map(|row| row.cell.clone()).collect::<Vec<_>>());
    Ok(json!({
        "binding": {"org_id": ctx.org.id, "user_id": ctx.user.id, "graph_id": ctx.row.graph.id},
        "policy": {"effects": policy.effects, "domains": policy.domains},
        "functions": cells.iter().filter(|row| row.cell.cell_type.has_function()).map(|row| row.cell.name.clone()).collect::<Vec<_>>(),
        "graphs": [GRAPH_NAME],
        "connectors": connector_grants(ctx, cells).await?,
    }))
}

/// The scheduler uses the same authenticated execution envelope as graph
/// registration; function input alone is never a policy or user binding.
pub async fn source_execution(ctx: &Ctx, function: &str) -> Result<Value, String> {
    let text = |e: ServerFnError| err_text(&e);
    let cells = cells_of(&ctx.row.graph.id).await.map_err(text)?;
    let cell = cells.iter().find(|row| row.cell.implementation.as_ref().and_then(|i| i.function.as_deref()) == Some(function))
        .ok_or("source function is not declared in this notebook")?;
    check_cell_refs(ctx, cell.cell.cell_type, &cell.cell.config).await.map_err(text)?;
    Ok(json!({
        "binding": {"org_id": ctx.org.id, "user_id": ctx.user.id, "graph_id": ctx.row.graph.id},
        "policy": super::db::org_settings(&ctx.org).await.map_err(text)?.effect_policy.for_cells(&cells.iter().map(|row| row.cell.clone()).collect::<Vec<_>>()),
        "connectors": connector_grants(ctx, &cells).await?,
        "liaisonUrl": config::runtime_liaison_url(),
    }))
}

/// Register the graph as a lun session, with the last value recorded for
/// each of its inputs: first registration and re-registration alike.
async fn register(ctx: &mut Ctx) -> Result<lun::SessionAnswer, String> {
    let text = |e: ServerFnError| err_text(&e);
    let build = ctx
        .row
        .session_build_id
        .clone()
        .ok_or("the notebook is not built")?;
    let structure = ctx
        .row
        .structure
        .clone()
        .ok_or("the notebook is not built")?;
    let recorded = latest_inputs(&ctx.row.graph.id).await.map_err(text)?;
    let inputs: Map<String, Value> = recorded
        .into_iter()
        .filter(|(k, _)| structure.inputs.contains(k))
        .collect();
    let cells = cells_of(&ctx.row.graph.id).await.map_err(text)?;
    if cells.iter().any(|c| c.cell.cell_type == CellType::DbSink) {
        if !super::db::org_settings(&ctx.org).await.map_err(text)?.effect_policy.effects.iter().any(|effect| effect == "PostgreSQL") {
            return Err("organization PostgreSQL effect denied".into());
        }
        compute::ensure(&ctx.org.id, &ctx.org.slug, &ctx.user.id).await?;
    }
    for c in cells
        .iter()
        .filter(|c| c.cell.cell_type == CellType::HttpSink)
    {
        if let Some(url) = &c.cell.config.url {
            check_public_url(url)
                .await
                .map_err(|e| format!("http sink {}: {e}", c.cell.name))?;
        }
    }
    let secrets: Vec<String> = cells
        .iter()
        .filter(|c| c.cell.cell_type == CellType::Secret && c.cell.secret_set)
        .filter_map(|c| c.cell.config.name.clone())
        .collect();
    let body = json!({
        "inputs": inputs,
        "recoverInputs": true,
        "binding": { "org_id": ctx.org.id, "user_id": ctx.user.id, "graph_id": ctx.row.graph.id },
        "secrets": secrets,
        "policy": super::db::org_settings(&ctx.org).await.map_err(text)?.effect_policy.for_cells(&cells.iter().map(|c| c.cell.clone()).collect::<Vec<_>>()),
        "warrants": storage_warrants(ctx, &cells).await?,
        "connectors": connector_grants(ctx, &cells).await?,
        "liaisonUrl": config::runtime_liaison_url(),
    });
    let answer = lun::start(&build, GRAPH_NAME, &body).await?;
    let pool = pool().map_err(text)?;
    sqlx::query(
        "update graphs set lun_session_id = $2, session_user_id = $3::uuid, last_nodes = $4::jsonb, \
         updated_at = now() where id = $1::uuid",
    )
    .bind(&ctx.row.graph.id)
    .bind(&answer.session)
    .bind(&ctx.user.id)
    .bind(serde_json::to_string(&answer.nodes).unwrap_or_else(|_| "[]".into()))
    .execute(pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;
    sqlx::query(
        "insert into graph_updates (graph_id, fed_by, changed) values ($1::uuid, 'register', $2::jsonb)",
    )
    .bind(&ctx.row.graph.id)
    .bind(serde_json::to_string(&answer.changed).unwrap_or_else(|_| "[]".into()))
    .execute(pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;
    ctx.row.lun_session_id = Some(answer.session.clone());
    ctx.row.session_user_id = Some(ctx.user.id.clone());
    Ok(answer)
}

/// Feed one input: record it, then update the session — registering it
/// again first if lun lost it — record what changed, and send what the
/// graph's `channel` sinks now say.
pub async fn feed(
    mut ctx: Ctx,
    input: &str,
    value: Value,
    fed_by: &str,
    cell_id: Option<&str>,
) -> Result<FeedResult, String> {
    let text = |e: ServerFnError| err_text(&e);
    // The session runs the adopted build, even while lode writes the next.
    if ctx.row.session_build_id.is_none() {
        return Err("the notebook is not built yet".to_string());
    }
    let structure = ctx
        .row
        .structure
        .clone()
        .ok_or("the notebook is not built yet")?;
    if !structure.inputs.iter().any(|i| i == input) {
        return Err(format!(
            "the graph has no input '{input}' (implement the notebook again)"
        ));
    }
    let pool = pool().map_err(text)?;
    sqlx::query(
        "insert into graph_inputs (graph_id, input, value, fed_by, cell_id) \
         values ($1::uuid, $2, $3::jsonb, $4, $5::uuid)",
    )
    .bind(&ctx.row.graph.id)
    .bind(input)
    .bind(value.to_string())
    .bind(fed_by)
    .bind(cell_id)
    .execute(pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;

    let cells = cells_of(&ctx.row.graph.id).await.map_err(text)?;
    let updated = match ctx.row.lun_session_id.clone() {
        Some(session) => {
            let mut inputs = Map::new();
            inputs.insert(input.to_string(), value);
            let body =
                json!({ "inputs": inputs, "warrants": storage_warrants(&ctx, &cells).await?,
                    "binding": { "org_id": ctx.org.id, "user_id": ctx.user.id, "graph_id": ctx.row.graph.id },
                    "policy": super::db::org_settings(&ctx.org).await.map_err(text)?.effect_policy.for_cells(&cells.iter().map(|c| c.cell.clone()).collect::<Vec<_>>()),
                    "connectors": connector_grants(&ctx, &cells).await?, "liaisonUrl": config::runtime_liaison_url() });
            lun::update(&session, &body).await?
        }
        None => None,
    };
    // Lost (or never registered): the recorded inputs, the new one included.
    let answer = match updated {
        Some(a) => a,
        None => register(&mut ctx).await?,
    };
    sqlx::query("update graphs set last_nodes = $2::jsonb, updated_at = now() where id = $1::uuid")
        .bind(&ctx.row.graph.id)
        .bind(serde_json::to_string(&answer.nodes).unwrap_or_else(|_| "[]".into()))
        .execute(pool)
        .await
        .map_err(|e| format!("database error: {e}"))?;
    sqlx::query(
        "insert into graph_updates (graph_id, fed_by, cell_id, changed) \
         values ($1::uuid, $2, $3::uuid, $4::jsonb)",
    )
    .bind(&ctx.row.graph.id)
    .bind(fed_by)
    .bind(cell_id)
    .bind(serde_json::to_string(&answer.changed).unwrap_or_else(|_| "[]".into()))
    .execute(pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;
    send_channel_sinks(&ctx, &cells, &answer.changed).await;
    let repair = auto_repair_errors(&ctx, &cells, &answer.changed, &answer.nodes).await;
    Ok(FeedResult {
        changed: answer.changed,
        nodes: answer.nodes,
        repair,
    })
}

/// The text a `channel` sink sends: its node's string output, or its JSON.
pub fn sink_text(output: &Value) -> Option<String> {
    let text = match output {
        Value::String(s) => s.trim().to_string(),
        Value::Null => return None,
        other => other.to_string(),
    };
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(4000).collect())
}

/// §4.5: a `channel` sink whose node's outcome changed sends its text,
/// through the same path as an inbox reply (liaison, a `write` warrant, the
/// message recorded with its sender). An unrelated recompute changes
/// nothing here, so sends nothing.
async fn send_channel_sinks(ctx: &Ctx, cells: &[CellRow], changed: &[GraphNode]) {
    for c in cells
        .iter()
        .filter(|c| c.cell.cell_type == CellType::ChannelSink)
    {
        let Some(function) = c
            .cell
            .implementation
            .as_ref()
            .and_then(|i| i.function.clone())
        else {
            continue;
        };
        let Some(output) = changed
            .iter()
            .find(|n| n.function.as_deref() == Some(function.as_str()))
            .and_then(|n| n.outcome.as_ref())
            .and_then(|o| o.output.as_ref())
        else {
            continue;
        };
        let (Some(text), Some(channel_id)) =
            (sink_text(output), c.cell.config.channel_id.as_deref())
        else {
            continue;
        };
        let recipient = c.cell.config.recipient.clone().unwrap_or_default();
        if let Err(e) = channels::send_scoped(
            &ctx.org,
            &ctx.user,
            &ctx.project.slug,
            channel_id,
            &recipient,
            &text,
            &c.cell.config.connectors,
            Some(&c.cell.id),
        )
        .await
        {
            eprintln!(
                "channel sink {} of graph {} could not send: {}",
                c.cell.name,
                ctx.row.graph.id,
                err_text(&e)
            );
        }
    }
}

/// A `ui` source's widget changed: feed it.
pub async fn feed_ui(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
    cell_id: &str,
    value: Value,
) -> Result<FeedResult, ServerFnError> {
    let ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    let cell = cell_by_id(&ctx.row.graph.id, cell_id).await?;
    if cell.cell.cell_type != CellType::UiInput {
        return Err(bad_request("not an input cell"));
    }
    let input = cell
        .cell
        .config
        .input
        .clone()
        .ok_or_else(|| bad_request("the cell has no input"))?;
    if !cell.cell.config.choices.is_empty()
        && !value
            .as_str()
            .is_some_and(|v| cell.cell.config.choices.iter().any(|c| c == v))
    {
        return Err(bad_request("not one of the choices"));
    }
    let input_type = cell
        .cell
        .implementation
        .as_ref()
        .and_then(|i| i.input_type.clone());
    json_fits(&value, input_type.as_deref()).map_err(bad_request)?;
    feed(ctx, &input, value, "ui", Some(&cell.cell.id))
        .await
        .map_err(bad_gateway)
}

/// Register the session again from the recorded inputs (the notebook's
/// "restart" — also what happens by itself when lun lost it).
pub async fn restart_session(
    org: &Org,
    user: &User,
    project_slug: &str,
    graph_slug: &str,
) -> Result<FeedResult, ServerFnError> {
    let mut ctx = member_ctx(org, user, project_slug, graph_slug).await?;
    if ctx.row.session_build_id.is_none() {
        return Err(conflict("the notebook is not built yet"));
    }
    if let Some(old) = ctx.row.lun_session_id.clone() {
        lun::end(&old).await;
    }
    let answer = register(&mut ctx).await.map_err(bad_gateway)?;
    if ctx.row.graph.status == "ready" {
        set_status(&ctx.row.graph.id, "ready", None).await?;
    }
    Ok(FeedResult {
        changed: answer.changed,
        nodes: answer.nodes,
        repair: None,
    })
}

// ── Channel sources (§3.6) ──────────────────────────────────────────────

/// A message just recorded on `channel_id` (inbound, new — retries record
/// nothing, so feed nothing): feed it to every ready graph whose `channel`
/// source listens there.
pub async fn on_channel_message(channel_id: &str, peer: &str, peer_name: Option<&str>, body: &str) {
    let Ok(pool) = pool() else { return };
    let rows = match sqlx::query(
        "select c.id::text as id, c.graph_id::text as graph_id, c.config ->> 'input' as input \
         from graph_cells c join graphs g on g.id = c.graph_id \
         where c.kind = 'source' and c.variant = 'channel' and c.config ->> 'channel_id' = $1 \
           and g.session_build_id is not null",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("could not look up channel sources of {channel_id}: {e}");
            return;
        }
    };
    let at = sqlx::query(
        "select to_char(now() at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as at",
    )
    .fetch_one(pool)
    .await
    .map(|r| r.get::<String, _>("at"))
    .unwrap_or_default();
    let message = json!({ "text": body, "peer": peer, "peer_name": peer_name, "at": at });
    for r in rows {
        let (cell_id, graph_id): (String, String) = (r.get("id"), r.get("graph_id"));
        let Some(input) = r.get::<Option<String>, _>("input") else {
            continue;
        };
        let result = match background_ctx(&graph_id).await {
            Ok(ctx) => feed(ctx, &input, message.clone(), "channel", Some(&cell_id)).await,
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            eprintln!("channel source {cell_id} could not feed graph {graph_id}: {e}");
        }
    }
}

// ── The SSRF guard (§3.1) ───────────────────────────────────────────────

/// Refuse a URL that is not `http(s)` or whose host resolves to private
/// address space — every address it resolves to must be public. (The fetch
/// itself happens in lun, which resolves again: a name that changes between
/// the two is the DNS-rebinding gap lun's own `HTTP` guard must close.)
pub async fn check_public_url(url: &str) -> Result<(), String> {
    crate::validate_fetch_url(url)?;
    let parsed = url::Url::parse(url).map_err(|e| format!("URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("URL: only http and https".to_string());
    }
    let port = parsed.port_or_known_default().unwrap_or(443);
    match parsed.host() {
        Some(url::Host::Ipv4(ip)) => {
            if crate::is_public_ip(ip.into()) {
                Ok(())
            } else {
                Err("URL: must be a public address".to_string())
            }
        }
        Some(url::Host::Ipv6(ip)) => {
            if crate::is_public_ip(ip.into()) {
                Ok(())
            } else {
                Err("URL: must be a public address".to_string())
            }
        }
        Some(url::Host::Domain(host)) => {
            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
                .await
                .map_err(|e| format!("URL: {host} does not resolve: {e}"))?
                .collect();
            if addrs.is_empty() {
                return Err(format!("URL: {host} does not resolve"));
            }
            if addrs.iter().any(|a| !crate::is_public_ip(a.ip())) {
                return Err(format!("URL: {host} resolves to a private address"));
            }
            Ok(())
        }
        None => Err("URL: a host".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(secret_path("o", "g", "api_key"), "graph/o/g/api_key");
        assert_eq!(project_path("invoices"), "typednotes/invoices");
    }

    #[test]
    fn stored_config_keeps_the_servers_fields() {
        let config = CellConfig {
            input: Some("x".into()),
            ..Default::default()
        };
        let keep = json!({"token_hash": "ab", "set_at": "now", "input": "old"});
        let stored = stored_config(&config, Some(&keep));
        assert_eq!(
            stored,
            json!({"input": "x", "token_hash": "ab", "set_at": "now"})
        );
        assert_eq!(stored_config(&config, None), json!({"input": "x"}));
    }

    #[test]
    fn sink_texts() {
        assert_eq!(sink_text(&json!(" total: 3 ")).as_deref(), Some("total: 3"));
        assert_eq!(sink_text(&json!({"a": 1})).as_deref(), Some("{\"a\":1}"));
        assert_eq!(sink_text(&json!("")), None);
        assert_eq!(sink_text(&Value::Null), None);
        assert_eq!(sink_text(&json!("x".repeat(5000))).unwrap().len(), 4000);
    }

    #[test]
    fn nodes_answer_like_lun() {
        let n = GraphNode {
            id: 2,
            function: Some("total".into()),
            args: vec![0, 1],
            outcome: Some(crate::NodeOutcome {
                output: Some(json!(7)),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            node_json(&n),
            json!({"id": 2, "function": "total", "args": [0, 1], "output": 7})
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_guard_refuses_literal_private_hosts() {
        assert!(check_public_url("http://127.0.0.1/").await.is_err());
        assert!(check_public_url("https://[fd00::1]/").await.is_err());
        assert!(check_public_url("ftp://example.com/").await.is_err());
    }
}
