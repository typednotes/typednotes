//! The app's client for `lun` (docs/services/lun.md, lun's README): submit
//! a build of the published commit, call a function of it, register a graph
//! as a session and update its inputs.
//!
//! lun's builds and sessions are reconstructible (docs/computations.md §2):
//! the same build request is the same build, and a session lun lost (a
//! `404`) is registered again from the app's input log.

use std::time::Duration;

use reqwest::Method;
use serde_json::{json, Value};

use super::config;
use super::rpc::{self, segment};
use crate::{parse_nodes, GraphNode};

pub fn configured() -> bool {
    config::lun().is_some()
}

fn service() -> Result<config::Service, String> {
    config::lun().ok_or_else(|| "the code runtime is not configured on this deployment".to_string())
}

fn fail(what: &str, a: &rpc::Answer) -> String {
    format!(
        "the code runtime refused {what} ({}): {}",
        a.status,
        a.error()
    )
}

/// A build, as lun reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub id: String,
    /// `queued`, `fetching`, `building`, `ready`, `failed`.
    pub state: String,
    pub error: Option<String>,
    pub diagnostics: Vec<String>,
    /// Once ready: the graph's inputs, nodes and structure.
    pub graph: Option<Structure>,
}

/// A graph of a ready build.
#[derive(Clone, Debug, PartialEq)]
pub struct Structure {
    pub inputs: Vec<String>,
    pub nodes: Vec<GraphNode>,
    pub sources: Vec<u64>,
    pub sinks: Vec<u64>,
}

impl Structure {
    pub fn to_json(&self) -> Value {
        json!({ "inputs": self.inputs, "sources": self.sources, "sinks": self.sinks })
    }

    /// The stored form (without nodes: those come with each answer).
    pub fn from_json(v: &Value) -> Structure {
        let ids = |k: &str| {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default()
        };
        Structure {
            inputs: v
                .get("inputs")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            nodes: Vec::new(),
            sources: ids("sources"),
            sinks: ids("sinks"),
        }
    }
}

/// A diagnostic, as one line: where (function, graph, project) and what.
fn diagnostic_line(d: &Value) -> String {
    let field = |k: &str| d.get(k).and_then(Value::as_str);
    let message = field("message")
        .map(str::to_string)
        .unwrap_or_else(|| d.to_string());
    let place = field("function")
        .map(|f| format!("function {f}"))
        .or_else(|| field("graph").map(|g| format!("graph {g}")))
        .or_else(|| field("scope").map(str::to_string));
    let first = message.lines().next().unwrap_or("").to_string();
    match place {
        Some(p) => format!("{p}: {first}"),
        None => first,
    }
}

/// Read a build's status, and the graph `graph` of it once ready.
pub fn parse_build(v: &Value, graph: &str) -> Build {
    let structure = v
        .get("graphs")
        .and_then(Value::as_array)
        .and_then(|gs| {
            gs.iter()
                .find(|g| g.get("name").and_then(Value::as_str) == Some(graph))
        })
        .map(|g| Structure {
            nodes: parse_nodes(g.get("nodes")),
            ..Structure::from_json(g)
        });
    Build {
        id: v
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        state: v
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        error: v.get("error").and_then(Value::as_str).map(str::to_string),
        diagnostics: v
            .get("diagnostics")
            .and_then(Value::as_array)
            .map(|ds| ds.iter().map(diagnostic_line).collect())
            .unwrap_or_default(),
        graph: structure,
    }
}

/// `POST /v0/builds`: `202` while it runs, `200` if already ready.
pub async fn submit(request: &Value, graph: &str) -> Result<Build, String> {
    let a = rpc::quick(&service()?, Method::POST, "/v0/builds", Some(request))
        .await
        .map_err(|e| format!("the code runtime: {e}"))?;
    if !a.ok() {
        return Err(fail("the build", &a));
    }
    Ok(parse_build(&a.body, graph))
}

/// `GET /v0/builds/{id}`; `None` if lun forgot it (a restart without
/// `LUN_ID_SALT`: submit it again).
pub async fn build(id: &str, graph: &str) -> Result<Option<Build>, String> {
    let a = rpc::quick(
        &service()?,
        Method::GET,
        &format!("/v0/builds/{}", segment(id)),
        None,
    )
    .await
    .map_err(|e| format!("the code runtime: {e}"))?;
    match a.status {
        404 => Ok(None),
        200 => Ok(Some(parse_build(&a.body, graph))),
        _ => Err(fail("the build status", &a)),
    }
}

/// `POST /v0/builds/{id}/functions/{name}` with one input: its output, or
/// the function's error.
pub async fn call_function(
    build: &str,
    name: &str,
    input: &Value,
    execution: &Value,
) -> Result<Result<Value, String>, String> {
    let path = format!("/v0/builds/{}/functions/{}", segment(build), segment(name));
    let mut body = execution.clone();
    let fields = body.as_object_mut().ok_or("execution context must be an object")?;
    if !fields.contains_key("binding") || !fields.contains_key("policy") {
        return Err("authenticated runtime policy and binding are required".into());
    }
    fields.insert("input".into(), input.clone());
    let a = rpc::call(
        &service()?,
        Method::POST,
        &path,
        Some(&body),
        Duration::from_secs(90),
    )
    .await
    .map_err(|e| format!("the code runtime: {e}"))?;
    if !a.ok() {
        return Err(fail("the call", &a));
    }
    if let Some(output) = a.body.get("output") {
        return Ok(Ok(output.clone()));
    }
    Ok(Err(a
        .body
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("the function answered nothing")
        .to_string()))
}

/// A session's answer: its id, every node, and those that changed.
pub struct SessionAnswer {
    pub session: String,
    pub nodes: Vec<GraphNode>,
    pub changed: Vec<GraphNode>,
}

/// `POST /v0/builds/{id}/graphs/{graph}/sessions`: register a session with
/// its inputs, its binding, its secrets' names and the warrants its storage
/// sinks write with (lun's computations contract, docs/services/lun.md §3).
pub async fn start(build: &str, graph: &str, body: &Value) -> Result<SessionAnswer, String> {
    let path = format!(
        "/v0/builds/{}/graphs/{}/sessions",
        segment(build),
        segment(graph)
    );
    let a = rpc::call(
        &service()?,
        Method::POST,
        &path,
        Some(body),
        Duration::from_secs(120),
    )
    .await
    .map_err(|e| format!("the code runtime: {e}"))?;
    if !a.ok() {
        return Err(fail("the session", &a));
    }
    let nodes = parse_nodes(a.body.get("nodes"));
    Ok(SessionAnswer {
        session: a
            .body
            .get("session")
            .and_then(Value::as_str)
            .ok_or("the code runtime's answer has no session id")?
            .to_string(),
        changed: nodes
            .iter()
            .filter(|n| n.outcome.is_some())
            .cloned()
            .collect(),
        nodes,
    })
}

/// `POST /v0/sessions/{id}`: feed some inputs. `None` if lun lost the
/// session (its container was recycled).
pub async fn update(session: &str, body: &Value) -> Result<Option<SessionAnswer>, String> {
    let path = format!("/v0/sessions/{}", segment(session));
    let a = rpc::call(
        &service()?,
        Method::POST,
        &path,
        Some(body),
        Duration::from_secs(120),
    )
    .await
    .map_err(|e| format!("the code runtime: {e}"))?;
    match a.status {
        404 => Ok(None),
        s if (200..300).contains(&s) => Ok(Some(SessionAnswer {
            session: session.to_string(),
            nodes: parse_nodes(a.body.get("nodes")),
            changed: parse_nodes(a.body.get("changed")),
        })),
        _ => Err(fail("the update", &a)),
    }
}

/// `DELETE /v0/sessions/{id}`, best-effort.
pub async fn end(session: &str) {
    if let Ok(svc) = service() {
        let path = format!("/v0/sessions/{}", segment(session));
        let _ = rpc::quick(&svc, Method::DELETE, &path, None).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_ready_build() {
        let b = parse_build(
            &json!({"id": "b1", "state": "ready", "diagnostics": [
                {"function": "total", "message": "unused variable\nmore"}, {"message": "note"}],
              "functions": [{"name": "total", "signature": "Nat → Eff [] Nat", "arity": 1}],
              "graphs": [{"name": "other"}, {"name": "main", "inputs": ["x"],
                 "nodes": [{"id": 0, "input": "x"}, {"id": 1, "function": "total", "args": [0]}],
                 "sources": [0], "sinks": [1]}]}),
            "main",
        );
        assert_eq!(b.state, "ready");
        assert_eq!(
            b.diagnostics,
            vec!["function total: unused variable", "note"]
        );
        let g = b.graph.unwrap();
        assert_eq!(g.inputs, vec!["x"]);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!((g.sources.clone(), g.sinks.clone()), (vec![0], vec![1]));
        assert_eq!(Structure::from_json(&g.to_json()).inputs, vec!["x"]);
        let failed = parse_build(
            &json!({"id": "b2", "state": "failed", "error": "no lakefile"}),
            "main",
        );
        assert_eq!(failed.error.as_deref(), Some("no lakefile"));
        assert!(failed.graph.is_none());
    }
}
