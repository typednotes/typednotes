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
    a.body
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "the code writer's answer has no session id".to_string())
}

/// `POST /v0/sessions/{id}/messages`: a run, or steering for the one going.
pub async fn message(id: &str, text: &str, credentials: Value) -> Result<Reply<()>, String> {
    let body = json!({ "text": text, "credentials": credentials });
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
pub async fn refresh(id: &str, credentials: Value) -> Result<Reply<()>, String> {
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
