//! The app's client for `lode` (docs/services/lode.md, lode's README): open
//! a session over the project's repository, send it messages (each with
//! fresh warrants — lode keeps them in memory only), follow its log.
//!
//! lode's sessions live on its container's disk and are reconstructible
//! (docs/computations.md §2): a `404` means the container was recycled, and
//! the caller opens a new session from the notebook.

use std::time::Duration;

use reqwest::Method;
use serde_json::{json, Value};

use super::config;
use super::rpc::{self, segment};
use crate::LodeEntry;

pub fn configured() -> bool {
    config::lode().is_some()
}

fn service() -> Result<config::Service, String> {
    config::lode().ok_or_else(|| "code writing is not configured on this deployment".to_string())
}

/// What a request about an existing session came to.
pub enum Reply<T> {
    Ok(T),
    /// lode no longer has the session.
    Gone,
}

fn fail(what: &str, a: &rpc::Answer) -> String {
    format!(
        "the code writer refused {what} ({}): {}",
        a.status,
        a.error()
    )
}

/// `POST /v0/sessions`: the new session's id.
pub async fn open(body: &Value) -> Result<String, String> {
    let a = rpc::quick(&service()?, Method::POST, "/v0/sessions", Some(body))
        .await
        .map_err(|e| format!("the code writer: {e}"))?;
    if !a.ok() {
        return Err(fail("the session", &a));
    }
    if let Some(expected)=body.get("buildContracts") {
        if a.body.get("buildContracts")!=Some(expected){return Err("the writer does not acknowledge caller-pinned build contracts; deploy Lode 0.4.3 or newer before generation".into());}
    }
    a.body
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "the code writer's answer has no session id".to_string())
}

/// `POST /v0/sessions/{id}/messages`: a run, or steering for the one going.
pub fn narrowing_tools(status: &Value, desired: &[String]) -> Result<Vec<String>, String> {
    let tools = status.get("tools").and_then(Value::as_array).ok_or("writer status has no tool policy")?;
    let mut current = Vec::new();
    for value in tools {
        let name = value.as_str().ok_or("invalid writer tool policy")?;
        if !crate::WRITER_TOOLS.contains(&name) || current.iter().any(|tool| tool == name) {
            return Err("unknown or duplicate writer tool policy".into());
        }
        current.push(name.to_string());
    }
    Ok(current.into_iter().filter(|tool| desired.contains(tool)).collect())
}

pub async fn message(id: &str, text: &str, credentials: Value, desired: &[String], execution: &Value) -> Result<Reply<()>, String> {
    let status = match status(id).await? { Reply::Gone => return Ok(Reply::Gone), Reply::Ok(status) => status };
    let tools = narrowing_tools(&status, desired)?;
    let execution = narrowing_execution(&status, execution)?;
    let body = json!({ "text": text, "credentials": credentials, "tools": tools, "execution": execution });
    let path = format!("/v0/sessions/{}/messages", segment(id));
    let a = rpc::quick(&service()?, Method::POST, &path, Some(&body))
        .await
        .map_err(|e| format!("the code writer: {e}"))?;
    match a.status {
        404 => Ok(Reply::Gone),
        s if (200..300).contains(&s) => Ok(Reply::Ok(())),
        _ => Err(fail("the message", &a)),
    }
}

/// `PUT /v0/sessions/{id}/credentials`: fresh warrants for a run in flight.
pub async fn refresh(id: &str, mut credentials: Value, desired: &[String], execution: &Value) -> Result<Reply<()>, String> {
    let status = match status(id).await? { Reply::Gone => return Ok(Reply::Gone), Reply::Ok(status) => status };
    let tools = narrowing_tools(&status, desired)?;
    let execution = narrowing_execution(&status, execution)?;
    // Policy edits are acknowledged on the message path, serialized with actual
    // execution. PUT can then replace operation tokens without changing ceilings.
    if status.get("tools") != Some(&json!(tools)) || public_execution(&execution)? != status.get("execution").cloned().unwrap_or(Value::Null) {
        let body = json!({"text": "Organization execution permissions narrowed. Continue within the allowed bounds.", "tools": tools, "execution": execution,"controlOnly":true});
        let a = rpc::quick(&service()?, Method::POST, &format!("/v0/sessions/{}/messages", segment(id)), Some(&body)).await?;
        if a.status == 404 { return Ok(Reply::Gone); }
        if !a.ok() { return Err(fail("execution narrowing", &a)); }
    }
    credentials["execution"] = execution;
    let path = format!("/v0/sessions/{}/credentials", segment(id));
    let a = rpc::quick(&service()?, Method::PUT, &path, Some(&credentials))
        .await
        .map_err(|e| format!("the code writer: {e}"))?;
    match a.status {
        404 => Ok(Reply::Gone),
        s if (200..300).contains(&s) => Ok(Reply::Ok(())),
        _ => Err(fail("the credentials", &a)),
    }
}

/// Public wire projection mirrors Lun's Session publicExecution. It is only
/// request shaping; Lean validates the transition and consumes its witness.
fn public_execution(execution: &Value) -> Result<Value, String> {
    let mut connectors = serde_json::Map::new();
    for (name, values) in execution.get("connectors").and_then(Value::as_object).ok_or("invalid writer execution grants")? {
        let mut grants = Vec::new();
        for value in values.as_array().ok_or("invalid writer execution grants")? {
            let mut grant = serde_json::Map::new();
            for key in ["provider", "connection", "account", "bucket", "organization", "connectionPermissions", "cell", "warrantPermissions"] {
                if let Some(value) = value.get(key) { grant.insert(key.into(), value.clone()); }
            }
            grants.push(Value::Object(grant));
        }
        connectors.insert(name.clone(), json!(grants));
    }
    Ok(json!({"execution": {"policy": execution["policy"], "binding": execution["binding"], "connectors": connectors},
        "functions": execution["functions"], "graphs": execution["graphs"]}))
}

/// Reuse the app's structured permissions intersection to keep fresh grants
/// below the writer's persisted current ceiling. Never infer authority from
/// compiled code, a selected project, an operator key, or a changed owner.
pub fn narrowing_execution(status: &Value, desired: &Value) -> Result<Value, String> {
    let bounds = status.get("execution").filter(|value| !value.is_null()).ok_or("writer has no trusted execution ceiling; open a new session")?;
    let old = bounds.get("execution").ok_or("invalid writer execution ceiling")?;
    if old.get("binding") != desired.get("binding") { return Err("writer actor/graph binding cannot change".into()); }
    let mut next = desired.clone();
    for key in ["effects", "domains"] {
        let old = old["policy"][key].as_array().ok_or("invalid writer policy ceiling")?;
        let requested = desired["policy"][key].as_array().ok_or("invalid writer policy")?;
        next["policy"][key] = json!(requested.iter().filter(|value| old.contains(value)).collect::<Vec<_>>());
    }
    for key in ["functions", "graphs"] {
        let old = bounds[key].as_array().ok_or("invalid writer service ceiling")?;
        let requested = desired[key].as_array().ok_or("invalid writer services")?;
        next[key] = json!(requested.iter().filter(|value| old.contains(value)).collect::<Vec<_>>());
    }
    let old_grants = old["connectors"].as_object().ok_or("invalid writer grant ceiling")?;
    let mut connectors = serde_json::Map::new();
    for (name, values) in desired["connectors"].as_object().ok_or("invalid writer grants")? {
        if !next["functions"].as_array().ok_or("invalid functions")?.contains(&json!(name)) { continue; }
        let mut grants = Vec::new();
        for value in values.as_array().ok_or("invalid writer grants")? {
            let previous = old_grants.get(name).and_then(Value::as_array).and_then(|grants| grants.iter().find(|old|
                ["provider", "connection", "account", "bucket"].iter().all(|key| old.get(*key) == value.get(*key))));
            let Some(previous) = previous else { continue; };
            let mut grant = value.clone();
            for key in ["organization", "connectionPermissions", "cell", "warrantPermissions"] {
                // Lun's documented omitted warrant ceiling equals the cell.
                let current = value.get(key).or_else(|| (key == "warrantPermissions").then(|| value.get("cell")).flatten()).ok_or("missing grant ceiling")?;
                let parent = previous.get(key).or_else(|| (key == "warrantPermissions").then(|| previous.get("cell")).flatten()).ok_or("missing grant ceiling")?;
                if current.get("provider") != parent.get("provider") || current.get("connection") != parent.get("connection") { return Err("grant identity changed".into()); }
                let permissions = |cap: &Value| serde_json::from_value::<crate::ConnectorPermissions>(json!({
                    "scopes": cap["scopes"], "maxRequestBytes": cap["maxRequestBytes"], "maxResponseBytes": cap["maxResponseBytes"]
                })).map_err(|_| "invalid grant permissions");
                let current = permissions(current)?;
                let parent_permissions = permissions(parent)?;
                let permissions = current.intersect(&parent_permissions);
                grant[key] = permissions.named_capability_json(value["provider"].as_str().ok_or("invalid provider")?, value["connection"].as_str().ok_or("invalid connection")?);
            }
            grants.push(grant);
        }
        connectors.insert(name.clone(), json!(grants));
    }
    next["connectors"] = Value::Object(connectors);
    Ok(next)
}

/// The final app binding transaction re-reads live policy/cell declarations.
/// Intersect a pre-minted envelope with that snapshot before model generation;
/// later policy edits see the registered writer and narrow it at Lode itself.
pub fn bind_execution(execution: &Value, live: &super::connector::WriterBinding) -> Result<Value, String> {
    let mut next = execution.clone();
    for (key, allowed) in [("effects", &live.policy.effects), ("domains", &live.policy.domains)] {
        let values = execution["policy"][key].as_array().ok_or("invalid writer execution policy")?;
        next["policy"][key] = json!(values.iter().filter(|value| value.as_str().is_some_and(|name| allowed.iter().any(|allowed| allowed == name))).collect::<Vec<_>>());
    }
    let functions = execution["functions"].as_array().ok_or("invalid writer functions")?;
    next["functions"] = json!(functions.iter().filter(|value| value.as_str().is_some_and(|name| live.functions.iter().any(|allowed| allowed == name))).collect::<Vec<_>>());
    next["connectors"].as_object_mut().ok_or("invalid writer grants")?.retain(|name, _| live.functions.contains(name));
    Ok(next)
}

/// Tool changes belong on messages. PUT credentials stays credentials-only;
/// asking for a larger organization list cannot restore removed session tools.
pub async fn narrow(id: &str, desired: &[String]) -> Result<Reply<()>, String> {
    let status = match status(id).await? { Reply::Gone => return Ok(Reply::Gone), Reply::Ok(status) => status };
    let tools = narrowing_tools(&status, desired)?;
    let mut body = json!({"text": "Organization writer permissions changed; outstanding effect authority is revoked. Continue within the allowed tools.", "tools": tools,"controlOnly":true});
    if let Some(bounds) = status.get("execution").filter(|value| !value.is_null()) {
        // This path runs under the org policy-edit transaction before commit.
        // Revoke every outstanding effect, including anonymous HTTP/files whose
        // authority has no broker row. A refresh cannot restore it in this writer.
        body["execution"] = json!({"policy": {"effects": [], "domains": []},
            "binding": bounds["execution"]["binding"], "connectors": {},
            "functions": bounds["functions"], "graphs": bounds["graphs"]});
    } else if status.get("tools") == Some(&json!(tools)) { return Ok(Reply::Ok(())); }
    let answer = rpc::quick(&service()?, Method::POST, &format!("/v0/sessions/{}/messages", segment(id)), Some(&body)).await?;
    match answer.status { 404 => Ok(Reply::Gone), 200..=299 => Ok(Reply::Ok(())), _ => Err(fail("tool narrowing", &answer)) }
}

/// `GET /v0/sessions/{id}`.
pub async fn status(id: &str) -> Result<Reply<Value>, String> {
    let path = format!("/v0/sessions/{}", segment(id));
    let a = rpc::quick(&service()?, Method::GET, &path, None)
        .await
        .map_err(|e| format!("the code writer: {e}"))?;
    match a.status {
        404 => Ok(Reply::Gone),
        200 => Ok(Reply::Ok(a.body)),
        _ => Err(fail("the status", &a)),
    }
}

/// Whether a status says a run is going (or about to).
pub fn running(status: &Value) -> bool {
    status.get("state").and_then(Value::as_str) == Some("running")
        || status.get("queued").and_then(Value::as_u64).unwrap_or(0) > 0
}

/// The commit lode last published or opened at.
pub fn head(status: &Value) -> Option<String> {
    status
        .pointer("/workspace/remoteHead")
        .and_then(Value::as_str)
        .filter(|h| !h.is_empty())
        .map(str::to_string)
}

/// `GET /v0/sessions/{id}/messages?after=n&wait=s`: new log entries,
/// summarised, the next index, and whether a run is going. `wait` holds the
/// request until something happens (at most 60 s on lode's side).
pub async fn log(
    id: &str,
    after: u64,
    wait: u64,
) -> Result<Reply<(Vec<LodeEntry>, u64, bool)>, String> {
    let wait = wait.min(25);
    let path = format!(
        "/v0/sessions/{}/messages?after={after}&wait={wait}",
        segment(id)
    );
    let a = rpc::call(
        &service()?,
        Method::GET,
        &path,
        None,
        Duration::from_secs(wait + 20),
    )
    .await
    .map_err(|e| format!("the code writer: {e}"))?;
    match a.status {
        404 => Ok(Reply::Gone),
        200 => {
            let entries = a
                .body
                .get("entries")
                .and_then(Value::as_array)
                .map(|es| es.iter().filter_map(summarise).collect())
                .unwrap_or_default();
            let next = a.body.get("next").and_then(Value::as_u64).unwrap_or(after);
            let running = a
                .body
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(Reply::Ok((entries, next, running)))
        }
        _ => Err(fail("the log", &a)),
    }
}

/// `GET /v0/sessions/{id}/diff`: what lode changed and has not published
/// yet, as a unified diff.
pub async fn diff(id: &str) -> Result<Reply<String>, String> {
    let svc = service()?;
    let response = super::oauth::http()
        .get(format!("{}/v0/sessions/{}/diff", svc.url, segment(id)))
        .bearer_auth(&svc.token)
        .send()
        .await
        .map_err(|e| format!("the code writer is unreachable: {e}"))?;
    match response.status().as_u16() {
        404 => Ok(Reply::Gone),
        200 => Ok(Reply::Ok(response.text().await.map_err(|e| {
            format!("the code writer's changes are unreadable: {e}")
        })?)),
        s => Err(format!("the code writer refused the changes ({s})")),
    }
}

/// `POST /v0/sessions/{id}/abort`.
pub async fn abort(id: &str) -> Result<(), String> {
    let path = format!("/v0/sessions/{}/abort", segment(id));
    let a = rpc::quick(&service()?, Method::POST, &path, None)
        .await
        .map_err(|e| format!("the code writer: {e}"))?;
    match a.status {
        s if (200..300).contains(&s) || s == 404 || s == 409 => Ok(()),
        _ => Err(fail("the abort", &a)),
    }
}

fn clip(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

/// A tool as the notebook names it: the services behind `lun_build` and
/// `lun_call` are not the user's concern.
fn tool_name(name: &str) -> &str {
    match name {
        "lun_build" => "build",
        "lun_call" => "try",
        other => other,
    }
}

/// One log entry as the notebook shows it (lode's README, "The log").
pub fn summarise(e: &Value) -> Option<LodeEntry> {
    let index = e.get("index")?.as_u64()?;
    let kind = e.get("type")?.as_str()?;
    let str_of = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let detail = entry_detail(e);
    let (kind, text) = match kind {
        // What the app sent: its first line (the rest is instructions for
        // the model, in the services' own terms).
        "user" => (
            "user",
            clip(str_of("text").lines().next().unwrap_or(""), 300),
        ),
        "assistant" => {
            let calls: Vec<String> = e
                .get("calls")
                .and_then(Value::as_array)
                .map(|cs| {
                    cs.iter()
                        .filter_map(|c| c.get("name").and_then(Value::as_str))
                        .map(|n| tool_name(n).to_string())
                        .collect()
                })
                .unwrap_or_default();
            let text = str_of("text");
            let text = match (text.trim().is_empty(), calls.is_empty()) {
                (_, true) => clip(&text, 2000),
                (true, false) => format!("→ {}", calls.join(", ")),
                (false, false) => format!("{} → {}", clip(&text, 1200), calls.join(", ")),
            };
            ("assistant", text)
        }
        "tool_results" => {
            let results: Vec<String> = e
                .get("results")
                .and_then(Value::as_array)
                .map(|rs| {
                    rs.iter()
                        .map(|r| {
                            let name =
                                tool_name(r.get("name").and_then(Value::as_str).unwrap_or("tool"));
                            let err = r.get("isError").and_then(Value::as_bool).unwrap_or(false);
                            let content = r.get("content").and_then(Value::as_str).unwrap_or("");
                            let first = content.lines().next().unwrap_or("");
                            format!(
                                "{name}{}: {}",
                                if err { " (error)" } else { "" },
                                clip(first, 160)
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            ("tool", results.join("\n"))
        }
        "compaction" => ("compaction", "earlier steps summarised".to_string()),
        "event" => {
            let detail = str_of("detail");
            let kind = str_of("kind");
            (
                "event",
                if detail.is_empty() {
                    kind
                } else {
                    format!("{kind}: {}", clip(&detail, 400))
                },
            )
        }
        _ => return None,
    };
    Some(LodeEntry {
        index,
        kind: kind.to_string(),
        text,
        detail,
        model: e.get("model").and_then(Value::as_str).map(str::to_string),
        usage: e.get("usage").map(|u| crate::TokenUsage {
            input: u.get("input").and_then(Value::as_u64).unwrap_or(0),
            output: u.get("output").and_then(Value::as_u64).unwrap_or(0),
            cache_read: u.get("cacheRead").and_then(Value::as_u64).unwrap_or(0),
            cache_write: u.get("cacheWrite").and_then(Value::as_u64).unwrap_or(0),
        }),
    })
}

/// The fuller view of an entry: each tool call with the file or command it
/// acts on and the code it writes, each tool result's content.
fn entry_detail(e: &Value) -> String {
    if e.get("type").and_then(Value::as_str) == Some("user") {
        return String::new();
    }
    let mut parts = Vec::new();
    for c in e
        .get("calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = tool_name(c.get("name").and_then(Value::as_str).unwrap_or("tool"));
        let args = c.get("arguments");
        let arg = |k: &str| args.and_then(|a| a.get(k)).and_then(Value::as_str);
        let target = arg("path").or(arg("command")).or(arg("name")).unwrap_or("");
        let mut part = format!("{name} {target}").trim_end().to_string();
        // The code a `write` or `edit` puts in.
        if let Some(code) = arg("content")
            .or(arg("new"))
            .or(arg("newText"))
            .or(arg("new_string"))
        {
            part.push('\n');
            part.push_str(&clip(code, 3000));
        }
        parts.push(part);
    }
    for r in e
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = tool_name(r.get("name").and_then(Value::as_str).unwrap_or("tool"));
        let content = r.get("content").and_then(Value::as_str).unwrap_or("");
        parts.push(format!("{name} →\n{}", clip(content, 2000)));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_binding_cannot_republish_a_stale_launch_policy_or_deleted_cell() {
        let execution = json!({"binding": {"org_id": "org", "user_id": "actor", "graph_id": "graph"},
            "policy": {"effects": ["HTTP", "Trace"], "domains": ["old.example.org"]},
            "functions": ["read", "removed"], "graphs": ["main"], "connectors": {"read": [], "removed": []}});
        let binding = super::super::connector::WriterBinding {
            policy: crate::EffectPolicy { effects: vec!["Trace".into()], domains: vec!["new.example.org".into()], ..Default::default() },
            functions: vec!["read".into(), "added".into()],
        };
        let next = bind_execution(&execution, &binding).unwrap();
        assert_eq!(next["policy"]["effects"], json!(["Trace"]));
        assert_eq!(next["policy"]["domains"], json!([]));
        assert_eq!(next["functions"], json!(["read"]));
        assert!(next["connectors"].get("removed").is_none());
        assert_eq!(next["binding"], execution["binding"]);
    }

    #[test]
    fn runtime_refresh_intersects_every_ceiling_and_retains_the_real_owner() {
        let cap = |root: &[&str]| json!({"provider": "s3", "connection": "connection", "scopes": [
            {"operation": "objects.read", "root": root, "descendants": true}],
            "maxRequestBytes": 1024, "maxResponseBytes": 2048});
        let grant = |root: &[&str]| json!({"provider": "s3", "connection": "connection", "account": "owner/connection",
            "bucket": null, "organization": cap(root), "connectionPermissions": cap(root), "cell": cap(root),
            "warrants": [{"operation": "objects.read", "warrant": {"fixture": "fresh-token"}, "cost": 0}]});
        let envelope = |root: &[&str]| json!({"binding": {"org_id": "org", "user_id": "actor", "graph_id": "graph"},
            "policy": {"effects": ["Connector"], "domains": []}, "functions": ["read"], "graphs": ["main"],
            "connectors": {"read": [grant(root)]}});
        let original = envelope(&["reports"]);
        let status = json!({"execution": public_execution(&original).unwrap()});
        let mut wider = envelope(&[]);
        wider["policy"]["effects"] = json!(["Connector", "HTTP"]);
        wider["functions"] = json!(["read", "extra"]);
        let narrowed = narrowing_execution(&status, &wider).unwrap();
        assert_eq!(narrowed["policy"]["effects"], json!(["Connector"]));
        assert_eq!(narrowed["functions"], json!(["read"]));
        for key in ["organization", "connectionPermissions", "cell", "warrantPermissions"] {
            assert_eq!(narrowed["connectors"]["read"][0][key], cap(&["reports"]));
        }
        assert_eq!(narrowed["connectors"]["read"][0]["account"], "owner/connection");
        assert_eq!(narrowed["connectors"]["read"][0]["warrants"], original["connectors"]["read"][0]["warrants"]);
        let mut substituted = original.clone();
        substituted["binding"]["user_id"] = json!("owner");
        assert!(narrowing_execution(&status, &substituted).is_err());
        substituted = original.clone();
        substituted["connectors"]["read"][0]["account"] = json!("actor/connection");
        assert!(narrowing_execution(&status, &substituted).unwrap()["connectors"]["read"].as_array().unwrap().is_empty());
        let public = public_execution(&narrowed).unwrap().to_string();
        assert!(!public.contains("fresh-token") && !public.contains("warrants"));
    }

    #[test]
    fn caller_tool_policy_only_attenuates_the_live_writer() {
        let current = json!({"tools": ["read", "todo"]});
        assert_eq!(narrowing_tools(&current, &["read".into(), "write".into(), "todo".into()]).unwrap(), ["read", "todo"]);
        assert!(narrowing_tools(&current, &[]).unwrap().is_empty());
        assert!(narrowing_tools(&json!({"tools": ["read", "read"]}), &["read".into()]).is_err());
        assert!(narrowing_tools(&json!({}), &["read".into()]).is_err());
    }

    #[test]
    fn summarises_the_log() {
        let a = summarise(&json!({"index": 3, "type": "assistant", "text": "",
            "calls": [{"id": "1", "name": "check", "arguments": {}}, {"id": "2", "name": "publish"}]}))
        .unwrap();
        assert_eq!(a.text, "→ check, publish");
        let w = summarise(&json!({"index": 5, "type": "assistant", "text": "",
            "calls": [{"id": "1", "name": "write", "arguments": {"path": "Sheet.lean", "content": "def total := 1"}}]}))
        .unwrap();
        assert_eq!(w.detail, "write Sheet.lean\ndef total := 1");
        let t = summarise(&json!({"index": 4, "type": "tool_results", "results": [
            {"id": "1", "name": "check", "content": "error: unknown identifier\nmore", "isError": true}]}))
        .unwrap();
        assert_eq!(t.kind, "tool");
        assert_eq!(t.text, "check (error): error: unknown identifier");
        let e = summarise(&json!({"index": 5, "type": "event", "kind": "run_finished"})).unwrap();
        assert_eq!(e.text, "run_finished");
        assert!(summarise(&json!({"index": 6, "type": "mystery"})).is_none());
    }

    /// The notebook shows the log: the services' names stay out of it.
    #[test]
    fn the_log_speaks_typednotes() {
        let u = summarise(&json!({"index": 0, "type": "user",
            "text": "Implement the typednotes notebook \"X\" in this directory.\n\nIt is a lun project."}))
        .unwrap();
        assert_eq!(
            u.text,
            "Implement the typednotes notebook \"X\" in this directory."
        );
        assert_eq!(u.detail, "");
        let a = summarise(&json!({"index": 1, "type": "assistant", "text": "",
            "calls": [{"id": "1", "name": "lun_build"}, {"id": "2", "name": "lun_call"}]}))
        .unwrap();
        assert_eq!(a.text, "→ build, try");
        let r = summarise(&json!({"index": 2, "type": "tool_results",
            "results": [{"id": "1", "name": "lun_build", "content": "ready"}]}))
        .unwrap();
        assert!(r.text.starts_with("build: ready") && r.detail.starts_with("build →"));
        for p in [
            crate::CellPhase::NoCode,
            crate::CellPhase::Writing,
            crate::CellPhase::Running,
            crate::CellPhase::Failed,
            crate::CellPhase::Stale,
        ] {
            assert!(!crate::mentions(p.label(), "lun") && !crate::mentions(p.label(), "lode"));
        }
        let first = crate::lode_message("X", &[], None);
        let first = first.lines().next().unwrap();
        assert!(!crate::mentions(first, "lun") && !crate::mentions(first, "lode"));
    }

    #[test]
    fn reads_the_status() {
        let s = json!({"state": "idle", "queued": 0, "workspace": {"remoteHead": "abc"}});
        assert!(!running(&s));
        assert_eq!(head(&s).as_deref(), Some("abc"));
        assert!(running(&json!({"state": "idle", "queued": 1})));
        assert!(running(&json!({"state": "running"})));
    }
}
