//! The source scheduler (docs/computations.md §3.1–3.2, §3.6): a tick, and
//! the due table in `graph_cells.next_due_at`.
//!
//! Every 30 seconds the tick claims the `scheduled` and `watch` cells of
//! ready notebooks that are due, moves each to its next cron time, and runs
//! it: the SSRF guard on its URL, the org's hourly cap, then the cell's
//! function on lun's build (the fetch happens there, in the function), and
//! its output fed to the cell's input — for a `watch`, only if it differs
//! from the last value fed. Each check is audited in `source_checks`. Signal
//! channels that `channel` sources listen on are pulled on the same tick.
//!
//! Claiming is a compare-and-set on `next_due_at`, so several app instances
//! never run the same check twice. The tick needs a live process: see §6
//! (`minScale := 1` once any org has a scheduled source).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sqlx::Row;

use super::db::pool;
use super::graphs::{self, background_ctx, check_public_url};
use super::{channels, config, lun};
use crate::{CellConfig, CellImpl, Cron};

const TICK: Duration = Duration::from_secs(30);

/// Start the tick, once per process. A no-op without lun: nothing could run.
pub fn start() {
    if !config::background_enabled() { return; }
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(async {
        // Let the server come up (and a sleeping database wake) first.
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            if lun::configured() {
                tick().await;
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// One pass: implementations in flight, due checks, then Signal pulls.
pub async fn tick() {
    recover_uncertain().await;
    graphs::tick_retirements().await;
    super::shares::cleanup().await;
    if super::lode::configured() {
        graphs::tick_queued().await;
        graphs::tick_implementing().await;
    }
    graphs::tick_runtime().await;
    graphs::tick_deliveries().await;
    match due().await {
        Ok(cells) => {
            for cell in cells {
                check(cell).await;
            }
        }
        Err(e) => eprintln!("scheduler: {e}"),
    }
    if let Ok(cells)=pending_sources().await {for cell in cells {check(cell).await;}}
    pull_signal().await;
}

/// A claimed check.
struct Due {
    run_id: String,
    cell_id: String,
    graph_id: String,
    org_id: String,
    watch: bool,
    config: CellConfig,
    implementation: Option<CellImpl>,
    build_id: String,
}

/// Claim the due cells: each moved to its next cron time, only if nobody
/// moved it first.
async fn due() -> Result<Vec<Due>, String> {
    let pool = pool().map_err(|e| super::errors::message(&e))?;
    let rows = sqlx::query(
        "select c.id::text as id, c.graph_id::text as graph_id, p.org_id::text as org_id, \
         c.variant, c.config::text as config, c.impl::text as impl, g.session_build_id, \
         extract(epoch from c.next_due_at)::bigint as due \
         from graph_cells c join graphs g on g.id = c.graph_id join projects p on p.id = g.project_id \
         where c.next_due_at <= now() and c.variant in ('scheduled', 'watch') \
            and g.session_build_id is not null and g.archived_at is null \
            and not exists(select 1 from scheduled_runs r where r.cell_id=c.id and r.state in ('queued','running','input_ready','uncertain')) \
         order by c.next_due_at limit 25",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;
    let now = super::warrant::now() as i64;
    let mut claimed = Vec::new();
    for r in rows {
        let config: CellConfig =
            serde_json::from_str(&r.get::<String, _>("config")).unwrap_or_default();
        let next = config
            .schedule
            .as_deref()
            .and_then(|s| Cron::parse(s).ok())
            .and_then(|c| c.next_after(now));
        let id: String = r.get("id");
        let old: i64 = r.get("due");
        let mut tx=pool.begin().await.map_err(|e|e.to_string())?;
        let moved = sqlx::query(
            "update graph_cells set next_due_at = to_timestamp($2::bigint) \
             where id = $1::uuid and extract(epoch from next_due_at)::bigint = $3",
        )
        .bind(&id)
        .bind(next)
        .bind(old)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("database error: {e}"))?;
        if moved.rows_affected() != 1 {
            continue;
        }
        let implementation: Option<String> = r.get("impl");
        let graph:String=r.get("graph_id");
        let declaration=json!({"config":config,"impl":implementation.as_deref().and_then(|s|serde_json::from_str::<Value>(s).ok())});
        let run=sqlx::query("insert into scheduled_runs(occurrence_key,graph_id,cell_id,actor_id,kind,due_at,build_id,declaration) select $1,g.id,$2::uuid,coalesce(g.session_user_id,g.created_by),'source',to_timestamp($3::bigint),$4,$5::jsonb from graphs g where g.id=$6::uuid on conflict do nothing returning id::text as id")
            .bind(format!("source:{id}:{old}")).bind(&id).bind(old).bind(r.get::<String,_>("session_build_id")).bind(declaration.to_string()).bind(&graph)
            .fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
        tx.commit().await.map_err(|e|e.to_string())?;
        let Some(run)=run else {continue;};
        claimed.push(Due {
            run_id:run.get("id"),
            cell_id: id,
            graph_id: r.get("graph_id"),
            org_id: r.get("org_id"),
            watch: r.get::<Option<String>, _>("variant").as_deref() == Some("watch"),
            config,
            implementation: implementation.and_then(|s| serde_json::from_str(&s).ok()),
            build_id: r.get("session_build_id"),
        });
    }
    Ok(claimed)
}

/// Queued occurrences survive a process restart. Re-read current code/config at
/// dispatch; edits or build replacement cancel obsolete work instead of guessing.
async fn pending_sources()->Result<Vec<Due>,String>{
    let pool=pool().map_err(|e|super::errors::message(&e))?;
    let rows=sqlx::query("select r.id::text as run,c.id::text as cell,g.id::text as graph,p.org_id::text as org,c.variant,c.config::text as config,c.impl::text as impl,r.build_id from scheduled_runs r join graph_cells c on c.id=r.cell_id join graphs g on g.id=r.graph_id join projects p on p.id=g.project_id where r.kind='source' and r.state in ('queued','input_ready') and not exists(select 1 from scheduled_runs blocked where blocked.graph_id=r.graph_id and blocked.state='uncertain') order by r.created_at limit 25")
        .fetch_all(pool).await.map_err(|e|e.to_string())?;
    rows.into_iter().map(|r|Ok(Due{run_id:r.get("run"),cell_id:r.get("cell"),graph_id:r.get("graph"),org_id:r.get("org"),watch:r.get::<String,_>("variant")=="watch",config:serde_json::from_str(&r.get::<String,_>("config")).map_err(|e|e.to_string())?,implementation:r.get::<Option<String>,_>("impl").map(|s|serde_json::from_str(&s)).transpose().map_err(|e|e.to_string())?,build_id:r.get("build_id")})).collect()
}

pub(super) enum RunResult {Completed,Failed,Uncertain,Cancelled}

pub(super) async fn finish_run(run:&str,result:RunResult,detail:&str){
    let Ok(pool)=pool()else{return;};
    let state=match result{RunResult::Completed=>"completed",RunResult::Failed=>"failed",RunResult::Uncertain=>"uncertain",RunResult::Cancelled=>"cancelled"};
    // A failure before dispatch is known not to have executed graph effects.
    // After dispatch, missing/ambiguous answers remain uncertain and are never
    // automatically replayed, even after the bounded claim lease expires.
    let _=sqlx::query("update scheduled_runs set state=case when $2='uncertain' and state in ('queued','input_ready') then 'failed' else $2 end,detail=$3,finished_at=now() where id=$1::uuid and state in ('queued','running','input_ready')")
        .bind(run).bind(state).bind(detail).execute(pool).await;
}

pub(super) async fn begin_run(run:&str,expected:&str,phase:&str)->Result<bool,String>{
    if !matches!((expected,phase),("queued","source")|("queued","graph")|("input_ready","graph")){return Err("invalid scheduled dispatch transition".into());}
    let mut tx=pool().map_err(|e|super::errors::message(&e))?.begin().await.map_err(|e|e.to_string())?;
    if phase=="source" {
        let org=sqlx::query("select p.org_id::text as org from scheduled_runs r join graphs g on g.id=r.graph_id join projects p on p.id=g.project_id where r.id=$1::uuid")
            .bind(run).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?.get::<String,_>("org");
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1,5))").bind(&org).execute(&mut *tx).await.map_err(|e|e.to_string())?;
        let used=sqlx::query("select (select count(*) from scheduled_runs r join graphs g on g.id=r.graph_id join projects p on p.id=g.project_id where p.org_id=$1::uuid and r.kind='source' and r.started_at>now()-interval '1 hour')+(select count(*) from source_checks where org_id=$1::uuid and started_at>now()-interval '1 hour' and latency_ms is not null and scheduled_run_id is null) as n")
            .bind(&org).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?.get::<i64,_>("n");
        if used>=config::source_checks_per_hour(){return Err("the organization's hourly source-check cap is reached".into());}
    }
    let result=sqlx::query("update scheduled_runs set state='running',phase=$3,started_at=coalesce(started_at,now()),phase_started_at=now() where id=$1::uuid and state=$2 and ((kind='producer' and $3='graph') or (kind='source' and (($2='queued' and $3='source') or ($2='input_ready' and $3='graph')))) and not exists(select 1 from scheduled_runs blocked where blocked.graph_id=scheduled_runs.graph_id and blocked.state='uncertain')")
        .bind(run).bind(expected).bind(phase).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    tx.commit().await.map_err(|e|e.to_string())?;
    Ok(result.rows_affected()==1)
}

async fn recover_uncertain(){
    let Ok(pool)=pool()else{return;};
    let _=sqlx::query("update scheduled_runs set state='uncertain',finished_at=now(),detail='Dispatch lease expired without a recorded response. Reconcile effects before explicitly restarting; no automatic replay.' where state='running' and coalesce(phase_started_at,started_at)<now()-interval '15 minutes'")
        .execute(pool).await;
}

async fn audit(d: &Due, latency: Option<Duration>, ok: bool, outcome: &str, fed: bool) {
    let Ok(pool) = pool() else { return };
    if let Err(e) = sqlx::query(
        "insert into source_checks (cell_id, org_id, latency_ms, ok, outcome, fed_at,scheduled_run_id) \
         values ($1::uuid, $2::uuid, $3, $4, $5, case when $6 then now() end,$7::uuid)",
    )
    .bind(&d.cell_id)
    .bind(&d.org_id)
    .bind(latency.map(|l| l.as_millis().min(i32::MAX as u128) as i32))
    .bind(ok)
    .bind(outcome)
    .bind(fed)
    .bind(&d.run_id)
    .execute(pool)
    .await
    {
        eprintln!(
            "scheduler: could not audit check of cell {}: {e}",
            d.cell_id
        );
    }
}

/// The value last fed to `input`, for a watch to compare with.
async fn last_fed(graph_id: &str, input: &str) -> Option<Value> {
    let pool = pool().ok()?;
    let row = sqlx::query(
        "select value::text as value from graph_inputs where graph_id = $1::uuid and input = $2 \
         order by at desc limit 1",
    )
    .bind(graph_id)
    .bind(input)
    .fetch_optional(pool)
    .await
    .ok()??;
    serde_json::from_str(&row.get::<String, _>("value")).ok()
}

/// What a watch does with a fresh value: feed it only if it changed.
pub fn watch_changed(last: Option<&Value>, fresh: &Value) -> bool {
    last != Some(fresh)
}

async fn check(d: Due) {
    if let Err(e)=check_run(&d).await {
        finish_run(&d.run_id,RunResult::Uncertain,&e).await;
        audit(&d,None,false,&e,false).await;
    }
}

async fn check_run(d:&Due)->Result<(),String>{
    let pool=pool().map_err(|e|super::errors::message(&e))?;
    let guard=graphs::runtime_lock(&d.graph_id).await?;
    let ctx=background_ctx(&d.graph_id).await?;
    let row=sqlx::query("select state,actor_id::text as actor,input_value::text as value,declaration=jsonb_build_object('config',c.config,'impl',c.impl) as same_declaration from scheduled_runs r join graph_cells c on c.id=r.cell_id where r.id=$1::uuid")
        .bind(&d.run_id).fetch_one(pool).await.map_err(|e|e.to_string())?;
    let state:String=row.get("state");
    if !matches!(state.as_str(),"queued"|"input_ready"){return Ok(());}
    if ctx.row.session_build_id.as_deref()!=Some(&d.build_id)||row.get::<Option<String>,_>("actor").as_deref()!=Some(&ctx.user.id)||!row.get::<bool,_>("same_declaration"){
        finish_run(&d.run_id,RunResult::Cancelled,"superseded by source declaration, actor or build change").await;return Ok(());
    }
    let (Some(url), Some(input)) = (d.config.url.clone(), d.config.input.clone()) else {
        return Err("the note has no URL or input".into());
    };
    let Some(function) = d.implementation.as_ref().and_then(|i| i.function.clone()) else {
        return Err("no code for this note yet".into());
    };
    let mut value=row.get::<Option<String>,_>("value").map(|s|serde_json::from_str::<Value>(&s)).transpose().map_err(|e|e.to_string())?;
    if state=="queued" {
    if let Err(e) = check_public_url(&url).await {
        return Err(format!("refused: {e}"));
    }
    let started = Instant::now();
    let execution=graphs::source_execution(&ctx,&function).await?;
    if !begin_run(&d.run_id,"queued","source").await?{return Ok(());}
    let called = lun::call_function(&d.build_id, &function, &json!(url), &execution).await;
    let latency = Some(started.elapsed());
    let fetched = match called {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => {
            finish_run(&d.run_id,RunResult::Failed,&e).await;
            audit(d,latency,false,&format!("the function failed: {e}"),false).await;
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    sqlx::query("update scheduled_runs set state='input_ready',input_value=$2::jsonb,detail='source response persisted; graph input pending' where id=$1::uuid and state='running'")
        .bind(&d.run_id).bind(fetched.to_string()).execute(pool).await.map_err(|e|e.to_string())?;
    value=Some(fetched);
    }
    let value=value.ok_or("persisted source occurrence has no result")?;
    if d.watch && !watch_changed(last_fed(&d.graph_id, &input).await.as_ref(), &value) {
        finish_run(&d.run_id,RunResult::Completed,"unchanged").await;
        audit(d,None,true,"unchanged",false).await;
        return Ok(());
    }
    guard.commit().await.map_err(|e|e.to_string())?;
    if !begin_run(&d.run_id,"input_ready","graph").await?{return Ok(());}
    let fed = match background_ctx(&d.graph_id).await {
        Ok(ctx) => graphs::feed_scheduled(ctx, &input, value, &d.cell_id,&d.run_id,&d.build_id).await,
        Err(e) => Err(e),
    };
    match fed {
        Ok(r) => {
            let n = r.changed.len();
            let outcome = format!(
                "{}: {n} note{} changed",
                if d.watch { "changed" } else { "fed" },
                if n == 1 { "" } else { "s" }
            );
            finish_run(&d.run_id,RunResult::Completed,&outcome).await;
            audit(d,None,true,&outcome,true).await;
        }
        Err(e) => return Err(format!("could not feed: {e}")),
    }
    Ok(())
}

/// Pull the Signal channels ready notebooks listen on; new messages feed
/// the graphs as they are recorded.
async fn pull_signal() {
    let Ok(pool) = pool() else { return };
    let rows = match sqlx::query(
        "select distinct ch.id::text as id from graph_cells c \
         join graphs g on g.id = c.graph_id \
         join channels ch on ch.id::text = c.config ->> 'channel_id' \
         where c.kind = 'source' and c.variant = 'channel' and ch.provider = 'signal' \
           and g.session_build_id is not null",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("scheduler: Signal channels: {e}");
            return;
        }
    };
    for r in rows {
        let id: String = r.get("id");
        if let Err(e) = channels::pull_signal_channel(&id).await {
            eprintln!("scheduler: Signal pull of {id} failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watch_feeds_only_a_change() {
        assert!(watch_changed(None, &json!(3)));
        assert!(watch_changed(Some(&json!(3)), &json!(4)));
        assert!(!watch_changed(
            Some(&json!({"price": 3})),
            &json!({"price": 3})
        ));
    }
}
