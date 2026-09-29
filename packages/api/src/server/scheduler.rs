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
    if super::lode::configured() {
        graphs::tick_implementing().await;
    }
    match due().await {
        Ok(cells) => {
            for cell in cells {
                check(cell).await;
            }
        }
        Err(e) => eprintln!("scheduler: {e}"),
    }
    pull_signal().await;
}

/// A claimed check.
struct Due {
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
           and g.session_build_id is not null \
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
        let moved = sqlx::query(
            "update graph_cells set next_due_at = to_timestamp($2::bigint) \
             where id = $1::uuid and extract(epoch from next_due_at)::bigint = $3",
        )
        .bind(&id)
        .bind(next)
        .bind(old)
        .execute(pool)
        .await
        .map_err(|e| format!("database error: {e}"))?;
        if moved.rows_affected() != 1 {
            continue;
        }
        let implementation: Option<String> = r.get("impl");
        claimed.push(Due {
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

async fn audit(d: &Due, latency: Option<Duration>, ok: bool, outcome: &str, fed: bool) {
    let Ok(pool) = pool() else { return };
    if let Err(e) = sqlx::query(
        "insert into source_checks (cell_id, org_id, latency_ms, ok, outcome, fed_at) \
         values ($1::uuid, $2::uuid, $3, $4, $5, case when $6 then now() end)",
    )
    .bind(&d.cell_id)
    .bind(&d.org_id)
    .bind(latency.map(|l| l.as_millis().min(i32::MAX as u128) as i32))
    .bind(ok)
    .bind(outcome)
    .bind(fed)
    .execute(pool)
    .await
    {
        eprintln!(
            "scheduler: could not audit check of cell {}: {e}",
            d.cell_id
        );
    }
}

/// Checks the org ran in the last hour (refusals, which ran nothing, have no
/// latency and do not count).
async fn checks_last_hour(org_id: &str) -> i64 {
    let Ok(pool) = pool() else { return 0 };
    sqlx::query(
        "select count(*) as n from source_checks where org_id = $1::uuid \
         and started_at > now() - interval '1 hour' and latency_ms is not null",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get("n"))
    .unwrap_or(0)
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
    let (Some(url), Some(input)) = (d.config.url.clone(), d.config.input.clone()) else {
        audit(&d, None, false, "the cell has no URL or input", false).await;
        return;
    };
    let Some(function) = d.implementation.as_ref().and_then(|i| i.function.clone()) else {
        audit(&d, None, false, "no code for this cell yet", false).await;
        return;
    };
    let cap = config::source_checks_per_hour();
    if checks_last_hour(&d.org_id).await >= cap {
        audit(
            &d,
            None,
            false,
            &format!("skipped: the org's cap of {cap} checks per hour is reached"),
            false,
        )
        .await;
        return;
    }
    if let Err(e) = check_public_url(&url).await {
        audit(&d, None, false, &format!("refused: {e}"), false).await;
        return;
    }
    let started = Instant::now();
    let called = lun::call_function(&d.build_id, &function, &json!(url)).await;
    let latency = Some(started.elapsed());
    let value = match called {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => {
            return audit(
                &d,
                latency,
                false,
                &format!("the function failed: {e}"),
                false,
            )
            .await
        }
        Err(e) => return audit(&d, latency, false, &e, false).await,
    };
    if d.watch && !watch_changed(last_fed(&d.graph_id, &input).await.as_ref(), &value) {
        return audit(&d, latency, true, "unchanged", false).await;
    }
    let fed = match background_ctx(&d.graph_id).await {
        Ok(ctx) => graphs::feed(ctx, &input, value, "check", Some(&d.cell_id)).await,
        Err(e) => Err(e),
    };
    match fed {
        Ok(r) => {
            let n = r.changed.len();
            let outcome = format!(
                "{}: {n} node{} changed",
                if d.watch { "changed" } else { "fed" },
                if n == 1 { "" } else { "s" }
            );
            audit(&d, latency, true, &outcome, true).await;
        }
        Err(e) => audit(&d, latency, false, &format!("could not feed: {e}"), false).await,
    }
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
