//! Computations (docs/computations.md): a graph is an ordered list of cells,
//! each a natural-language description of one node of a reactive graph that
//! `lode` implements and `lun` runs.
//!
//! Everything here is shared by client and server: the types they exchange,
//! and the pure rules both apply — cell validation, cron schedules, the
//! reading of `lun.json` and of lun's answers, a graph's dependency closure,
//! which widget an input gets, and the message sent to `lode`. None of it
//! talks to a service.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Channel, Org, Project};

// ── Cells ───────────────────────────────────────────────────────────────

/// What a cell is: a plain node, one of the six sources (§3) or one of the
/// five sinks (§4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CellType {
    Node,
    Scheduled,
    Watch,
    UiInput,
    Secret,
    Endpoint,
    ChannelSource,
    DbSink,
    HttpSink,
    StorageSink,
    UiSink,
    ChannelSink,
}

impl CellType {
    pub const ALL: [CellType; 12] = [
        CellType::Node,
        CellType::Scheduled,
        CellType::Watch,
        CellType::UiInput,
        CellType::Secret,
        CellType::Endpoint,
        CellType::ChannelSource,
        CellType::DbSink,
        CellType::HttpSink,
        CellType::StorageSink,
        CellType::UiSink,
        CellType::ChannelSink,
    ];

    /// `graph_cells.kind`.
    pub fn kind(self) -> &'static str {
        match self {
            CellType::Node => "node",
            CellType::Scheduled
            | CellType::Watch
            | CellType::UiInput
            | CellType::Secret
            | CellType::Endpoint
            | CellType::ChannelSource => "source",
            _ => "sink",
        }
    }

    /// `graph_cells.variant`: the source's trigger or the sink's kind.
    pub fn variant(self) -> Option<&'static str> {
        match self {
            CellType::Node => None,
            CellType::Scheduled => Some("scheduled"),
            CellType::Watch => Some("watch"),
            CellType::UiInput | CellType::UiSink => Some("ui"),
            CellType::Secret => Some("secret"),
            CellType::Endpoint => Some("endpoint"),
            CellType::ChannelSource | CellType::ChannelSink => Some("channel"),
            CellType::DbSink => Some("db"),
            CellType::HttpSink => Some("http"),
            CellType::StorageSink => Some("storage"),
        }
    }

    pub fn from_db(kind: &str, variant: Option<&str>) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.kind() == kind && t.variant() == variant)
    }

    /// A stable id, for menus.
    pub fn id(self) -> String {
        match self.variant() {
            None => "node".to_string(),
            Some(v) => format!("{}-{v}", self.kind()),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CellType::Node => "Node",
            CellType::Scheduled => "Source · scheduled fetch",
            CellType::Watch => "Source · watch a page",
            CellType::UiInput => "Source · input in the notebook",
            CellType::Secret => "Source · secret",
            CellType::Endpoint => "Source · webhook endpoint",
            CellType::ChannelSource => "Source · channel message",
            CellType::DbSink => "Sink · database table",
            CellType::HttpSink => "Sink · HTTP POST",
            CellType::StorageSink => "Sink · storage",
            CellType::UiSink => "Sink · notebook output",
            CellType::ChannelSink => "Sink · channel message",
        }
    }

    pub fn is_source(self) -> bool {
        self.kind() == "source"
    }

    pub fn is_sink(self) -> bool {
        self.kind() == "sink"
    }

    /// Whether lode implements the cell as a declared function. A `ui`,
    /// `endpoint` or `channel` source is an input with a trigger, and a
    /// secret a capability (§1.1).
    pub fn has_function(self) -> bool {
        !matches!(
            self,
            CellType::UiInput | CellType::Secret | CellType::Endpoint | CellType::ChannelSource
        )
    }

    /// Whether the cell feeds a named input of the graph.
    pub fn has_input(self) -> bool {
        matches!(
            self,
            CellType::Scheduled
                | CellType::Watch
                | CellType::UiInput
                | CellType::Endpoint
                | CellType::ChannelSource
        )
    }

    /// Whether the cell's function is applied in the graph (a scheduled or
    /// watch source's function runs outside it, on the app's tick, and feeds
    /// its input).
    pub fn in_graph(self) -> bool {
        self.has_function() && !matches!(self, CellType::Scheduled | CellType::Watch)
    }

    /// The effect row the function declares (§1.1), as lode is told.
    pub fn effects(self) -> &'static str {
        match self {
            CellType::Scheduled | CellType::Watch | CellType::HttpSink => "[HTTP]",
            CellType::DbSink => "[PostgreSQL]",
            CellType::StorageSink => "[ObjectStore]",
            _ => "[]",
        }
    }
}

/// A cell's structured config, next to its prose (§3, §4). Which fields
/// apply depends on the [`CellType`]; the others are `None`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connectors: Vec<crate::CellConnector>,
    /// Named cell arguments, in order. None preserves pre-declaration notebooks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
    /// Optional Lean result type; argument types come from dependencies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<String>,
    /// `scheduled`, `watch`: the page; `http` sink: where to POST.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// `scheduled`, `watch`: five-field cron, UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// The graph input a source feeds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// `ui` source: a closed set of string values, shown as a select.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// `secret`: its name (never its value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `channel` source or sink: one of the project's interfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    /// `channel` sink: who a WhatsApp or Signal message goes to (Slack posts
    /// to the channel itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,
    /// `db` sink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    /// `storage` sink: one of the org's storage connections, and the path in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// `ui` sink: `table`, `chart`, `markdown`, or anything else (raw JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// What lode produced for a cell, read from the published `lun.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellImpl {
    /// The declared function (`lun.json`'s `name`).
    pub function: Option<String>,
    /// The Lean constant it is (`lun.json`'s `function`, e.g. `Sheet.total`).
    #[serde(default)]
    pub constant: Option<String>,
    pub module: Option<String>,
    pub signature: Option<String>,
    /// The effect row, e.g. `["HTTP"]`.
    pub effects: Vec<String>,
    /// The graph input the cell feeds, and its Lean type.
    pub input: Option<String>,
    pub input_type: Option<String>,
}

/// A node's outcome in lun's answer: its value, its own error, or the
/// argument without a value it was skipped for.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeOutcome {
    pub output: Option<Value>,
    pub error: Option<String>,
    pub skipped: Option<u64>,
}

/// A node of the graph, as lun reports it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: u64,
    pub input: Option<String>,
    pub function: Option<String>,
    pub args: Vec<u64>,
    /// `None` until something reached it.
    pub outcome: Option<NodeOutcome>,
}

/// One cell of a notebook.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub id: String,
    pub position: i32,
    /// The identifier lode names the cell's function (and, by default, its
    /// input) after.
    pub name: String,
    pub cell_type: CellType,
    pub description: String,
    pub config: CellConfig,
    pub implementation: Option<CellImpl>,
    /// `secret` cells: whether a value was set (never the value).
    pub secret_set: bool,
    /// `endpoint` cells: whether a token exists (the URL is shown once).
    pub has_endpoint: bool,
    /// Sources: the value last fed to their input.
    pub last_input: Option<Value>,
    /// `scheduled`, `watch`: the next check, and the last one's outcome.
    pub next_due_at: Option<String>,
    pub last_check: Option<String>,
    /// lode is (re)writing this cell's code right now.
    pub writing: bool,
    /// Why the code is being (or was last) rewritten.
    pub issue: Option<String>,
    /// The prose or config changed since the current code was written.
    pub stale: bool,
}

/// Where a cell is in its life: lode writes its code from the description,
/// then lun runs it; a failure or a member's report sends it back to lode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellPhase {
    /// No code yet, and no run writing it.
    NoCode,
    /// lode is writing (or rewriting) the code.
    Writing,
    /// The code runs on lun.
    Running,
    /// lun reported an error for the cell's node.
    Failed,
    /// The code runs, but the description changed since it was written.
    Stale,
}

impl CellPhase {
    pub fn label(self) -> &'static str {
        match self {
            CellPhase::NoCode => "no code yet",
            CellPhase::Writing => "writing the code",
            CellPhase::Running => "running",
            CellPhase::Failed => "the code failed",
            CellPhase::Stale => "code older than the description",
        }
    }

    /// The CSS modifier.
    pub fn id(self) -> &'static str {
        match self {
            CellPhase::NoCode => "none",
            CellPhase::Writing => "writing",
            CellPhase::Running => "running",
            CellPhase::Failed => "failed",
            CellPhase::Stale => "stale",
        }
    }
}

/// A cell's phase, from its row, whether lode is running for the notebook,
/// and its node's outcome.
pub fn cell_phase(cell: &Cell, implementing: bool, node: Option<&GraphNode>) -> CellPhase {
    if cell.writing && implementing {
        return CellPhase::Writing;
    }
    let has_code = match &cell.implementation {
        None => false,
        Some(i) => !cell.cell_type.has_function() || i.function.is_some(),
    };
    if !has_code {
        return CellPhase::NoCode;
    }
    if node
        .and_then(|n| n.outcome.as_ref())
        .is_some_and(|o| o.error.is_some())
    {
        return CellPhase::Failed;
    }
    if cell.stale {
        CellPhase::Stale
    } else {
        CellPhase::Running
    }
}

/// A notebook, as listed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub id: String,
    pub slug: String,
    pub name: String,
    /// `editing`, `implementing`, `ready` or `failed`.
    pub status: String,
    /// Why it failed, or what it is doing.
    pub detail: Option<String>,
    /// The published commit lun built.
    pub commit: Option<String>,
    pub build_id: Option<String>,
    /// Whether a lun session is registered.
    pub session: bool,
    /// The AI connection lode's model calls go through.
    pub model_connection_id: Option<String>,
    pub model_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Where lode's current (or last) run starts in its log.
    pub log_start: u64,
}

/// What happened lately in a graph, newest first: updates, checks and
/// endpoint deliveries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub at: String,
    /// `update`, `check` or `endpoint`.
    pub kind: String,
    pub cell: Option<String>,
    pub ok: bool,
    pub text: String,
}

/// A notebook page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphDetail {
    pub org: Org,
    pub project: Project,
    pub graph: Graph,
    pub cells: Vec<Cell>,
    /// The graph's nodes with their last outcomes (empty before a build).
    pub nodes: Vec<GraphNode>,
    /// lun's reading of the graph's structure: nodes reading nothing, and
    /// nodes nothing reads.
    pub sources: Vec<u64>,
    pub sinks: Vec<u64>,
    /// The project's interfaces, for `channel` cells.
    pub channels: Vec<Channel>,
    pub activity: Vec<Activity>,
    /// Whether the caller may edit (every member may; kept for the UI).
    pub can_edit: bool,
    #[serde(default)]
    pub effect_policy: crate::EffectPolicy,
}

/// A cell just created or re-keyed: an `endpoint` cell's full URL is shown
/// this once and never again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellSaved {
    pub cell: Cell,
    pub endpoint_url: Option<String>,
}

/// One entry of lode's log, summarised for the notebook.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LodeEntry {
    pub index: u64,
    /// `user`, `assistant`, `tool`, `event`, `compaction`.
    pub kind: String,
    pub text: String,
    /// More of it, for a cell's view of the writing: tool arguments (the
    /// file written, the code), fuller tool results.
    pub detail: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

/// Where an implementation run is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LodeProgress {
    pub graph: Graph,
    /// lode's `idle`/`running`, or `none` without a session.
    pub state: String,
    pub entries: Vec<LodeEntry>,
    pub next: u64,
}

/// The answer to feeding an input: the nodes that changed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FeedResult {
    pub changed: Vec<GraphNode>,
    pub nodes: Vec<GraphNode>,
    /// Set when a node's error sent its cell back to lode (the notebook,
    /// now implementing).
    pub repair: Option<Graph>,
}

/// A cell's code: what the published commit holds for its function, and —
/// while lode writes — the unpublished changes that mention it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellCode {
    /// The module's file in the repository.
    pub path: Option<String>,
    pub published: Option<String>,
    pub draft: Option<String>,
}

// ── Validation ──────────────────────────────────────────────────────────

/// A cell name, function name or input name: a Lean-friendly identifier,
/// `[a-z][a-z0-9_]*`, at most 40 characters.
pub fn validate_ident(what: &str, s: &str) -> Result<String, String> {
    let s = s.trim();
    let ok = (1..=40).contains(&s.len())
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(s.to_string())
    } else {
        Err(format!(
            "{what}: 1–40 lowercase letters, digits and underscores, starting with a letter"
        ))
    }
}

/// A cell's description: 1–4000 characters of prose.
pub fn validate_description(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() || s.chars().count() > 4000 {
        Err("description: 1–4000 characters".to_string())
    } else {
        Ok(s.to_string())
    }
}

/// Whether an address is one a source or sink may reach: not loopback,
/// private, link-local, carrier-grade NAT, multicast, documentation,
/// unspecified or broadcast space (the SSRF guard of §3.1).
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let seg = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00 // unique local
                || (seg[0] & 0xffc0) == 0xfe80 // link-local
                || (seg[0] & 0xffc0) == 0xfec0 // site-local (deprecated)
                || (seg[0] == 0x2001 && seg[1] == 0x0db8) // documentation
                || (seg[0] == 0x0064 && seg[1] == 0xff9b) // NAT64
                || seg[0] == 0x2002) // 6to4, which embeds any v4
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b)) // carrier-grade NAT
        || (a == 192 && b == 0 && c == 0) // IETF protocol assignments
        || (a == 198 && (b == 18 || b == 19)) // benchmarking
        || a >= 240) // reserved
}

/// A URL a source fetches or an `http` sink posts to: `http(s)://host…`,
/// no credentials, no fragment, and not a literal private address. The
/// server also resolves the host before every use (a name can point
/// anywhere); this is the part both sides can check.
pub fn validate_fetch_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or("URL: must start with https:// or http://")?;
    if url.len() > 2000
        || url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "\\\"<>{}|^`".contains(c))
        || url.contains('#')
    {
        return Err("URL: no spaces, fragments or quotes".to_string());
    }
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err("URL: a host, without credentials".to_string());
    }
    let host = host_of(authority);
    if host.is_empty() {
        return Err("URL: a host".to_string());
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") || lower.ends_with(".internal") {
        return Err("URL: must be a public address".to_string());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err("URL: must be a public address".to_string());
        }
    }
    Ok(url.to_string())
}

/// The host of a URL authority (`host`, `host:port`, `[v6]:port`).
pub fn host_of(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or("");
    }
    authority.rsplit_once(':').map_or(authority, |(h, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) {
            h
        } else {
            authority
        }
    })
}

/// A secret's name: a path segment, `[A-Za-z0-9_-]`, 1–64 characters.
pub fn validate_secret_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(name.to_string())
    } else {
        Err("secret name: 1–64 letters, digits, `_` and `-`".to_string())
    }
}

/// A secret's value: 1–8192 characters.
pub fn validate_secret_value(value: &str) -> Result<String, String> {
    if value.is_empty() || value.chars().count() > 8192 {
        Err("secret: 1–8192 characters".to_string())
    } else {
        Ok(value.to_string())
    }
}

/// A path inside a storage connection: `/`-separated segments, none empty,
/// `.` or `..`, at most 300 characters.
pub fn validate_storage_path(path: &str) -> Result<String, String> {
    let path = path.trim().trim_matches('/');
    let ok = (1..=300).contains(&path.len())
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && !s.chars().any(char::is_control))
        && !path.contains('\\');
    if ok {
        Ok(path.to_string())
    } else {
        Err("path: segments separated by /, without . or ..".to_string())
    }
}

/// Check a cell before it is stored; returns the normalized name,
/// description and config. `input` defaults to the cell's name.
pub fn validate_cell(
    cell_type: CellType,
    name: &str,
    description: &str,
    config: &CellConfig,
) -> Result<(String, String, CellConfig), String> {
    let name = validate_ident("name", name)?;
    let description = validate_description(description)?;
    let trimmed = |o: &Option<String>| {
        o.as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let input = || match trimmed(&config.input) {
        Some(i) => validate_ident("input", &i),
        None => Ok(name.clone()),
    };
    let required = |o: &Option<String>, what: &str| trimmed(o).ok_or(format!("{what}: required"));
    let mut out = CellConfig::default();
    if config.connectors.len() > 32 || config.connectors.iter().map(|c| &c.connection).collect::<BTreeSet<_>>().len() != config.connectors.len() {
        return Err("connectors: at most 32 distinct connections".into());
    }
    out.connectors = config.connectors.clone();
    out.dependencies = config.dependencies.as_ref().map(|deps| {
        deps.iter().map(|name| validate_ident("dependency", name)).collect::<Result<Vec<_>, _>>()
    }).transpose()?;
    if let Some(deps) = &out.dependencies {
        if deps.len() > 64 || deps.iter().collect::<BTreeSet<_>>().len() != deps.len() {
            return Err("dependencies: at most 64 distinct cell names".into());
        }
        if cell_type.is_source() && !deps.is_empty() {
            return Err("sources do not take cell dependencies".into());
        }
    }
    out.output_type = trimmed(&config.output_type);
    if let Some(ty) = &out.output_type {
        if ty.len() > 512 || ty.chars().any(|c| c.is_control() || c == ';')
            || ty.contains("--") || ty.contains("/-") || ty.contains('#') {
            return Err("output type: one Lean type expression, at most 512 characters".into());
        }
    }
    match cell_type {
        CellType::Node => {}
        CellType::Scheduled | CellType::Watch => {
            out.url = Some(validate_fetch_url(&required(&config.url, "URL")?)?);
            let schedule = required(&config.schedule, "schedule")?;
            Cron::parse(&schedule)?;
            out.schedule = Some(schedule);
            out.input = Some(input()?);
        }
        CellType::UiInput => {
            out.input = Some(input()?);
            let mut choices = Vec::new();
            for c in &config.choices {
                let c = c.trim();
                if c.is_empty() {
                    continue;
                }
                if c.chars().count() > 100 {
                    return Err("choices: at most 100 characters each".to_string());
                }
                if !choices.iter().any(|x: &String| x == c) {
                    choices.push(c.to_string());
                }
            }
            if choices.len() > 50 {
                return Err("choices: at most 50".to_string());
            }
            out.choices = choices;
        }
        CellType::Secret => {
            out.name = Some(validate_secret_name(&required(
                &config.name,
                "secret name",
            )?)?);
        }
        CellType::Endpoint => out.input = Some(input()?),
        CellType::ChannelSource => {
            out.channel_id = Some(required(&config.channel_id, "interface")?);
            out.input = Some(input()?);
        }
        CellType::DbSink => {
            let table = required(&config.table, "table")?;
            out.table = Some(validate_ident("table", &table)?);
        }
        CellType::HttpSink => {
            out.url = Some(validate_fetch_url(&required(&config.url, "URL")?)?);
        }
        CellType::StorageSink => {
            out.connection_id = Some(required(&config.connection_id, "storage connection")?);
            out.path = Some(validate_storage_path(&required(&config.path, "path")?)?);
        }
        CellType::UiSink => {
            let format = trimmed(&config.format).unwrap_or_else(|| "json".to_string());
            if format.len() > 20 || !format.chars().all(|c| c.is_ascii_lowercase()) {
                return Err("format: table, chart, markdown or json".to_string());
            }
            out.format = Some(format);
        }
        CellType::ChannelSink => {
            out.channel_id = Some(required(&config.channel_id, "interface")?);
            out.recipient = trimmed(&config.recipient);
        }
    }
    Ok((name, description, out))
}

/// The UI sink formats the notebook renders; any other renders as JSON.
pub const UI_FORMATS: [&str; 4] = ["table", "chart", "markdown", "json"];

// ── Cron ────────────────────────────────────────────────────────────────

/// A five-field cron expression (`minute hour day-of-month month
/// day-of-week`), evaluated in UTC. Fields take `*`, `n`, `a-b`, `*/s`,
/// `a-b/s` and comma lists; day-of-week is `0`–`7` (both `0` and `7` are
/// Sunday). When both day fields are restricted a day matches either, as in
/// every cron. `@hourly`, `@daily`, `@weekly` and `@monthly` are accepted.
#[derive(Clone, Debug, PartialEq)]
pub struct Cron {
    minutes: u64,
    hours: u32,
    days: u32,
    months: u16,
    weekdays: u8,
    days_any: bool,
    weekdays_any: bool,
}

fn cron_field(field: &str, min: u32, max: u32) -> Result<(u64, bool), String> {
    let mut bits = 0u64;
    let any = field == "*";
    for part in field.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (
                r,
                s.parse::<u32>()
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or(format!("bad step in '{part}'"))?,
            ),
            None => (part, 1),
        };
        let num = |s: &str| {
            s.parse::<u32>()
                .ok()
                .filter(|n| (min..=max).contains(n))
                .ok_or(format!("'{s}' is outside {min}–{max}"))
        };
        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            let (a, b) = (num(a)?, num(b)?);
            if a > b {
                return Err(format!("empty range '{range}'"));
            }
            (a, b)
        } else {
            let n = num(range)?;
            (n, if part.contains('/') { max } else { n })
        };
        let mut v = lo;
        while v <= hi {
            bits |= 1 << v;
            v += step;
        }
    }
    Ok((bits, any))
}

/// Days since 1970-01-01 → (year, month 1–12, day 1–31). Howard Hinnant's
/// `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

impl Cron {
    pub fn parse(expr: &str) -> Result<Cron, String> {
        let expr = expr.trim();
        let expanded = match expr {
            "@hourly" => "0 * * * *",
            "@daily" | "@midnight" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            other => other,
        };
        let fields: Vec<&str> = expanded.split_whitespace().collect();
        let [m, h, dom, mon, dow] = fields[..] else {
            return Err(
                "schedule: five cron fields (minute hour day month weekday), e.g. 0 9 * * 1-5"
                    .to_string(),
            );
        };
        fn err(what: &'static str) -> impl Fn(String) -> String {
            move |e| format!("schedule: {what}: {e}")
        }
        let (minutes, _) = cron_field(m, 0, 59).map_err(err("minute"))?;
        let (hours, _) = cron_field(h, 0, 23).map_err(err("hour"))?;
        let (days, days_any) = cron_field(dom, 1, 31).map_err(err("day of month"))?;
        let (months, _) = cron_field(mon, 1, 12).map_err(err("month"))?;
        let (mut weekdays, weekdays_any) = cron_field(dow, 0, 7).map_err(err("day of week"))?;
        if weekdays & (1 << 7) != 0 {
            weekdays |= 1;
        }
        Ok(Cron {
            minutes,
            hours: hours as u32,
            days: days as u32,
            months: months as u16,
            weekdays: (weekdays & 0x7f) as u8,
            days_any,
            weekdays_any,
        })
    }

    fn day_matches(&self, days_since_epoch: i64) -> bool {
        let (_, month, day) = civil_from_days(days_since_epoch);
        if self.months & (1 << month) == 0 {
            return false;
        }
        // 1970-01-01 was a Thursday (4).
        let weekday = (days_since_epoch + 4).rem_euclid(7) as u32;
        let dom = self.days & (1 << day) != 0;
        let dow = self.weekdays & (1 << weekday) != 0;
        match (self.days_any, self.weekdays_any) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => dom || dow,
        }
    }

    /// The first due time strictly after `after` (Unix seconds), on a whole
    /// minute; `None` if nothing matches within eight years (`0 0 30 2 *`).
    pub fn next_after(&self, after: i64) -> Option<i64> {
        let start = after.div_euclid(60) + 1; // the next whole minute
        let first_day = start.div_euclid(1440);
        for day in first_day..first_day + 8 * 366 {
            if !self.day_matches(day) {
                continue;
            }
            let from = if day == first_day {
                start.rem_euclid(1440)
            } else {
                0
            };
            for minute in from..1440 {
                let (h, m) = (minute / 60, minute % 60);
                if self.hours & (1 << h) != 0 && self.minutes & (1 << m) != 0 {
                    return Some((day * 1440 + minute) * 60);
                }
            }
        }
        None
    }
}

// ── lun.json and lun's answers ──────────────────────────────────────────

/// A declared function of `lun.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LunFunction {
    pub name: String,
    pub module: String,
    pub function: String,
    pub signature: String,
}

/// A graph of `lun.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LunGraph {
    pub name: String,
    pub program: String,
}

/// `lun.json`, as lode writes it next to the code.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LunJson {
    #[serde(default)]
    pub open: Vec<String>,
    #[serde(default)]
    pub functions: Vec<LunFunction>,
    #[serde(default)]
    pub graphs: Vec<LunGraph>,
}

/// The graph every notebook is built as.
pub const GRAPH_NAME: &str = "main";

/// The `input "name" Type` bindings of a graph program, in order.
pub fn program_inputs(program: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in program.lines() {
        let mut rest = line;
        while let Some(at) = rest.find("input \"") {
            let before = &rest[..at];
            // `input` as a word, not the end of another identifier.
            let word_start = before
                .chars()
                .last()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '.'));
            let after = &rest[at + 7..];
            let Some(close) = after.find('"') else { break };
            let name = &after[..close];
            let ty = after[close + 1..].trim();
            // The type runs to the end of the line, minus a trailing comment.
            let ty = ty.split("--").next().unwrap_or("").trim();
            if word_start && !name.is_empty() && !ty.is_empty() {
                out.push((name.to_string(), ty.to_string()));
            }
            rest = &after[close + 1..];
        }
    }
    out
}

/// The effect row of a signature `… → Eff [E₁ …, E₂ …] B`: each effect's head
/// name (`HTTP`, `PostgreSQL`…), namespaces dropped.
pub fn signature_effects(signature: &str) -> Vec<String> {
    let Some(at) = signature.find("Eff") else {
        return Vec::new();
    };
    let rest = &signature[at + 3..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let mut depth = 0i32;
    let mut current = String::new();
    let mut effects = Vec::new();
    for c in rest[open + 1..].chars() {
        match c {
            '[' | '(' => {
                depth += 1;
                current.push(c);
            }
            ']' if depth == 0 => break,
            ']' | ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                effects.push(std::mem::take(&mut current));
            }
            c => current.push(c),
        }
    }
    effects.push(current);
    effects
        .iter()
        .filter_map(|e| e.split_whitespace().next())
        .map(|head| head.trim_matches(['(', ')']))
        .map(|head| head.rsplit('.').next().unwrap_or(head).to_string())
        .filter(|h| !h.is_empty())
        .collect()
}

/// What lode produced for `cell`, from the published `lun.json`.
pub fn cell_impl(cell_type: CellType, name: &str, config: &CellConfig, lun: &LunJson) -> CellImpl {
    let function = lun
        .functions
        .iter()
        .find(|f| cell_type.has_function() && f.name == name);
    let input = config.input.clone().filter(|_| cell_type.has_input());
    let input_type = input.as_ref().and_then(|input| {
        lun.graphs
            .iter()
            .filter(|g| g.name == GRAPH_NAME)
            .flat_map(|g| program_inputs(&g.program))
            .find(|(n, _)| n == input)
            .map(|(_, t)| t)
    });
    CellImpl {
        function: function.map(|f| f.name.clone()),
        constant: function.map(|f| f.function.clone()),
        module: function.map(|f| f.module.clone()),
        signature: function.map(|f| f.signature.clone()),
        effects: function
            .map(|f| signature_effects(&f.signature))
            .unwrap_or_default(),
        input,
        input_type,
    }
}

/// A node of lun's answer (`{"id", "input"|"function", "args", "output"|"error"|"skipped"}`).
pub fn parse_node(v: &Value) -> Option<GraphNode> {
    let id = v.get("id")?.as_u64()?;
    let outcome = if let Some(output) = v.get("output") {
        Some(NodeOutcome {
            output: Some(output.clone()),
            ..Default::default()
        })
    } else if let Some(e) = v.get("error") {
        Some(NodeOutcome {
            error: Some(
                e.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| e.to_string()),
            ),
            ..Default::default()
        })
    } else {
        v.get("skipped")
            .and_then(Value::as_u64)
            .map(|s| NodeOutcome {
                skipped: Some(s),
                ..Default::default()
            })
    };
    Some(GraphNode {
        id,
        input: v.get("input").and_then(Value::as_str).map(str::to_string),
        function: v
            .get("function")
            .and_then(Value::as_str)
            .map(str::to_string),
        args: v
            .get("args")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default(),
        outcome,
    })
}

pub fn parse_nodes(v: Option<&Value>) -> Vec<GraphNode> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(parse_node).collect())
        .unwrap_or_default()
}

/// The nodes a node reads directly, and every node it depends on
/// (transitively), each ascending.
pub fn dependencies(nodes: &[GraphNode], id: u64) -> (Vec<u64>, Vec<u64>) {
    let args_of = |id: u64| {
        nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.args.clone())
            .unwrap_or_default()
    };
    let direct: BTreeSet<u64> = args_of(id).into_iter().collect();
    let mut closure = BTreeSet::new();
    let mut stack: Vec<u64> = direct.iter().copied().collect();
    while let Some(n) = stack.pop() {
        if closure.insert(n) {
            stack.extend(args_of(n));
        }
    }
    (direct.into_iter().collect(), closure.into_iter().collect())
}

/// The graph node standing for a cell: its function's application, or the
/// input it feeds.
pub fn node_of<'a>(nodes: &'a [GraphNode], cell: &Cell) -> Option<&'a GraphNode> {
    let imp = cell.implementation.as_ref();
    let function = imp
        .and_then(|i| i.function.as_deref())
        .filter(|_| cell.cell_type.in_graph());
    let input = imp
        .and_then(|i| i.input.as_deref())
        .or(cell.config.input.as_deref())
        .filter(|_| cell.cell_type.has_input());
    nodes.iter().find(|n| match (function, input) {
        (Some(f), _) => n.function.as_deref() == Some(f),
        (None, Some(i)) => n.input.as_deref() == Some(i),
        _ => false,
    })
}

/// How a node reads in the notebook: `#3 total` or `#0 input country`.
pub fn node_label(n: &GraphNode) -> String {
    match (&n.function, &n.input) {
        (Some(f), _) => format!("#{} {f}", n.id),
        (None, Some(i)) => format!("#{} input {i}", n.id),
        _ => format!("#{}", n.id),
    }
}

// ── Inputs: widgets and types ───────────────────────────────────────────

/// The widget an input gets, from its JSON type (§3.3).
#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    /// A number; `integer` for `Nat`/`Int`, `min` 0 for `Nat`.
    Number {
        integer: bool,
        natural: bool,
    },
    Text,
    Toggle,
    Select(Vec<String>),
    /// Anything else: its JSON, edited as text.
    Json,
}

pub fn widget_for(input_type: Option<&str>, choices: &[String]) -> Widget {
    if !choices.is_empty() {
        return Widget::Select(choices.to_vec());
    }
    match input_type.map(str::trim) {
        Some("Nat") => Widget::Number {
            integer: true,
            natural: true,
        },
        Some("Int") => Widget::Number {
            integer: true,
            natural: false,
        },
        Some("Float") => Widget::Number {
            integer: false,
            natural: false,
        },
        Some("String") => Widget::Text,
        Some("Bool") => Widget::Toggle,
        _ => Widget::Json,
    }
}

/// Whether `value` is plausibly of Lean type `ty` as `FromJson` reads it.
/// Only the base types are checked; anything else is lun's to refuse.
pub fn json_fits(value: &Value, ty: Option<&str>) -> Result<(), String> {
    let fits = match ty.map(str::trim) {
        Some("Nat") => value.as_u64().is_some(),
        Some("Int") => value.as_i64().is_some() || value.as_u64().is_some(),
        Some("Float") => value.is_number(),
        Some("String") => value.is_string(),
        Some("Bool") => value.is_boolean(),
        Some(t) if t.starts_with("List ") || t.starts_with("Array ") => value.is_array(),
        _ => true,
    };
    if fits {
        Ok(())
    } else {
        Err(format!(
            "the body is not a {}",
            ty.unwrap_or("value of the input's type")
        ))
    }
}

// ── The message to lode ─────────────────────────────────────────────────

/// What a cell's config says, for lode's prompt and the notebook.
pub fn describe_config(cell_type: CellType, c: &CellConfig) -> String {
    let opt = |o: &Option<String>| o.clone().unwrap_or_default();
    match cell_type {
        CellType::Node => String::new(),
        CellType::Scheduled => format!(
            "fetch {} on schedule `{}`, feeding input `{}`",
            opt(&c.url),
            opt(&c.schedule),
            opt(&c.input)
        ),
        CellType::Watch => format!(
            "watch {} on schedule `{}`; a change feeds input `{}`",
            opt(&c.url),
            opt(&c.schedule),
            opt(&c.input)
        ),
        CellType::UiInput if c.choices.is_empty() => format!("input `{}`", opt(&c.input)),
        CellType::UiInput => format!("input `{}`, one of {}", opt(&c.input), c.choices.join(", ")),
        CellType::Secret => format!("secret `{}`", opt(&c.name)),
        CellType::Endpoint => format!("webhook feeding input `{}`", opt(&c.input)),
        CellType::ChannelSource => format!("channel messages feeding input `{}`", opt(&c.input)),
        CellType::DbSink => format!("writes table `{}`", opt(&c.table)),
        CellType::HttpSink => format!("POSTs to {}", opt(&c.url)),
        CellType::StorageSink => format!("writes `{}` in a storage connection", opt(&c.path)),
        CellType::UiSink => format!("shown as {}", c.format.as_deref().unwrap_or("json")),
        CellType::ChannelSink => "sent as a message on an interface".to_string(),
    }
}

/// The instruction for one cell, in the message to lode.
fn cell_instruction(cell: &Cell) -> String {
    let n = &cell.name;
    let input = cell.config.input.clone().unwrap_or_else(|| n.clone());
    let instruction = match cell.cell_type {
        CellType::Node => format!(
            "a function `{n}` with effect row `[]`, applied in the graph to the values it needs"
        ),
        CellType::Scheduled | CellType::Watch => format!(
            "a function `{n}` of signature `String → Eff [HTTP] T`: it receives the page URL, \
             fetches it and returns the extracted value; it is NOT applied in the graph — the \
             app calls it on its schedule and feeds its result to the graph input \
             `input \"{input}\" T`{}",
            if cell.cell_type == CellType::Watch {
                " (return only the salient part, so an unrelated change of the page is no change)"
            } else {
                ""
            }
        ),
        CellType::UiInput => format!(
            "no function: a graph input `input \"{input}\" T`, set by the user in the notebook{}",
            if cell.config.choices.is_empty() {
                String::new()
            } else {
                format!(" (a String, one of: {})", cell.config.choices.join(", "))
            }
        ),
        CellType::Secret => format!(
            "no function and no input: the secret `{}`, read with the `SecretStore` effect by \
             the functions that need it (declare `SecretStore` in their rows); never return, \
             log or store its value",
            cell.config.name.clone().unwrap_or_default()
        ),
        CellType::Endpoint => format!(
            "no function: a graph input `input \"{input}\" T` fed with the JSON body of webhook \
             requests"
        ),
        CellType::ChannelSource => format!(
            "no function: a graph input `input \"{input}\" Message` fed with each message, the \
             JSON object {{\"text\": String, \"peer\": String, \"peer_name\": Option String, \
             \"at\": String}}; parse it downstream"
        ),
        CellType::DbSink => format!(
            "a function `{n}` with `PostgreSQL` in its row, writing the table `{}` (the \
             connection target and actor schema come from the trusted non-secret compute target supplied by the app; never name a password)",
            cell.config.table.clone().unwrap_or_default()
        ),
        CellType::HttpSink => format!(
            "a function `{n}` with `HTTP` in its row that POSTs its argument, as JSON, to {} \
             and returns the response status",
            cell.config.url.clone().unwrap_or_default()
        ),
        CellType::StorageSink => format!(
            "a function `{n}` with `ObjectStore` in its row writing its result at `{}` (the \
             bucket is the runtime's: name none)",
            cell.config.path.clone().unwrap_or_default()
        ),
        CellType::UiSink => format!(
            "a function `{n}` with effect row `[]` returning JSON for the `{}` renderer{}",
            cell.config.format.as_deref().unwrap_or("json"),
            match cell.config.format.as_deref() {
                Some("table") => ": an array of objects with the same keys",
                Some("chart") => {
                    ": {\"type\": \"line\"|\"bar\", \"labels\": [String], \
                     \"series\": [{\"name\": String, \"values\": [Float]}]}"
                }
                Some("markdown") => ": a String of Markdown (no HTML)",
                _ => "",
            }
        ),
        CellType::ChannelSink => format!(
            "a function `{n}` with effect row `[]` returning the message text as a String (the \
             app sends it when it changes)"
        ),
    };
    let deps = cell.config.dependencies.clone().unwrap_or_else(|| crate::cell_references(&cell.description));
    let named = if cell.config.dependencies.is_some() || !deps.is_empty() {
        if deps.is_empty() {
            " This cell takes no cell arguments (a zero-dependency function is a source).".into()
        } else {
            format!(" Its arguments are exactly [{}], in that order; infer each argument type from the named cell's output. No undeclared cell dependencies.", deps.join(", "))
        }
    } else { String::new() };
    let output = cell.config.output_type.as_ref().map(|ty|
        format!(" The output type is constrained to `{ty}`; declare that exact result type, inside `Eff effs ({ty})` for functions. It will be checked by the runtime.")
    ).unwrap_or_default();
    let external: Vec<_> = cell.config.connectors.iter().filter(|grant| crate::LocalService::from_selection(&grant.connection).is_none()).collect();
    let local: Vec<_> = cell.config.connectors.iter().filter(|grant| crate::LocalService::from_selection(&grant.connection).is_some()).collect();
    let connectors = if external.is_empty() { String::new() } else {
        format!(" This cell may use only the following connected-service grants through `Control.Monad.Effect.Connector`, in its `Eff` row: {}. Use named operations and structured resource components; never raw HTTP or IO to bypass a connector grant.", serde_json::to_string(&external).unwrap_or_default())
    };
    let locals = if local.is_empty() { String::new() } else {
        format!(" Bound local-service requests: {}. Use the PostgreSQL or SecretStore effect and the trusted actor/graph bindings; these are not external Connector credentials. No raw SQL or raw vault HTTP.", serde_json::to_string(&local).unwrap_or_default())
    };
    format!("{instruction}{named}{output}{connectors}{locals}")
}

/// The message that asks lode to implement the notebook: the cells in
/// order, each with its instruction, the contract every cell shares, and
/// the user's steering note if any.
pub fn lode_message(graph_name: &str, cells: &[Cell], note: Option<&str>) -> String {
    let mut out = format!(
        "Implement the typednotes notebook \"{graph_name}\" in this directory.\n\n\
         It is a lun project. Each cell below is a natural-language description of one node. Implement every cell as \
         instructed, write `lun.json` declaring every function under its exact name and one graph \
         named `{GRAPH_NAME}` wiring them (inputs with `input \"name\" Type`, then the functions \
         applied to what they need), publish one commit, `lun_build` it, and fix what lun reports \
         until the build is ready and the functions answer as described. Every argument and result \
         is a JSON value (FromJson/ToJson). Keep existing functions whose cells did not change.\n\n\
         Cells, in order:\n"
    );
    for (i, cell) in cells.iter().enumerate() {
        out.push_str(&format!(
            "\n{}. `{}` — {}\n   Description: {}\n   Implement: {}.\n",
            i + 1,
            cell.name,
            cell.cell_type.label(),
            cell.description.replace('\n', "\n   "),
            cell_instruction(cell),
        ));
    }
    if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
        out.push_str(&format!("\nFrom the user: {note}\n"));
    }
    out
}

// ── Rewrites (lode, again) ──────────────────────────────────────────────

/// Whether `text` mentions the identifier `name` as a whole word.
pub fn mentions(text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
    let mut from = 0;
    while let Some(at) = text[from..].find(name) {
        let start = from + at;
        let end = start + name.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(ident) && !after.is_some_and(ident) {
            return true;
        }
        from = end;
    }
    false
}

/// The message that sends one cell back to lode: its description, what it
/// should be, what it is now, and what is wrong with it.
pub fn repair_message(graph_name: &str, cell: &Cell, issue: &str) -> String {
    let current = match cell.implementation.as_ref() {
        Some(CellImpl {
            function: Some(f),
            signature: Some(sig),
            module,
            ..
        }) => format!(
            "\nCurrently: `{f} : {sig}`{}.",
            module
                .as_ref()
                .map(|m| format!(" in `{m}`"))
                .unwrap_or_default()
        ),
        _ => String::new(),
    };
    format!(
        "In the typednotes notebook \"{graph_name}\", the cell `{name}` ({label}) needs its code \
         rewritten.\n\nDescription: {description}\nImplement: {instruction}.{current}\n\n\
         What is wrong: {issue}\n\nFix it, and whatever depends on it; keep every other \
         function's name and signature and the graph named `{GRAPH_NAME}`. Publish one commit, \
         `lun_build` it, and check with `lun_call` that the cell now does what its description \
         says.\n",
        name = cell.name,
        label = cell.cell_type.label(),
        description = cell.description.replace('\n', "\n  "),
        instruction = cell_instruction(cell),
    )
}

/// The message that sends a failed build back to lode, naming the cells
/// lun's diagnostics point at.
pub fn build_repair_message(graph_name: &str, cells: &[&Cell], diagnostics: &str) -> String {
    let mut out = format!(
        "The published commit of the typednotes notebook \"{graph_name}\" did not build.\n\n\
         lun's diagnostics:\n\n{diagnostics}\n\nFix the code, publish one commit, `lun_build` it until it is ready, and \
         check the functions with `lun_call`. The cells concerned:\n"
    );
    for cell in cells {
        out.push_str(&format!(
            "\n- `{}` ({}): {}\n  Implement: {}.\n",
            cell.name,
            cell.cell_type.label(),
            cell.description.replace('\n', "\n  "),
            cell_instruction(cell)
        ));
    }
    out
}

/// The file of a Lean module (`Sheet.Prices` → `Sheet/Prices.lean`), if its
/// name is plain identifiers.
pub fn module_file(module: &str) -> Option<String> {
    let parts: Vec<&str> = module.split('.').collect();
    let plain = |p: &&str| {
        !p.is_empty()
            && p.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !p.starts_with(|c: char| c.is_ascii_digit())
    };
    (parts.len() <= 20 && parts.iter().all(plain)).then(|| format!("{}.lean", parts.join("/")))
}

/// Whether a line starts a new top-level declaration or block.
fn starts_decl(line: &str) -> bool {
    const HEADS: [&str; 22] = [
        "def ",
        "theorem ",
        "lemma ",
        "abbrev ",
        "structure ",
        "inductive ",
        "class ",
        "instance ",
        "namespace ",
        "end ",
        "section",
        "open ",
        "variable ",
        "#",
        "@[",
        "/--",
        "private ",
        "protected ",
        "noncomputable ",
        "partial ",
        "unsafe ",
        "example",
    ];
    !line.starts_with([' ', '\t']) && HEADS.iter().any(|h| line.starts_with(h))
}

/// The definition of `function` (a full name, `Sheet.total`) in a module's
/// source: its doc comment, header and body, up to the next top-level
/// declaration.
pub fn extract_definition(source: &str, function: &str) -> Option<String> {
    let name = function.rsplit('.').next()?;
    let lines: Vec<&str> = source.lines().collect();
    let is_header = |l: &str| {
        let mut rest = l;
        for m in [
            "private ",
            "protected ",
            "noncomputable ",
            "partial ",
            "unsafe ",
        ] {
            rest = rest.strip_prefix(m).unwrap_or(rest);
        }
        ["def ", "abbrev ", "theorem "].iter().any(|k| {
            rest.strip_prefix(k).is_some_and(|r| {
                let r = r.trim_start();
                let head = r
                    .split(|c: char| {
                        c.is_whitespace() || c == ':' || c == '(' || c == '{' || c == '['
                    })
                    .next()
                    .unwrap_or("");
                head == name || head.ends_with(&format!(".{name}"))
            })
        })
    };
    let at = lines.iter().position(|l| is_header(l))?;
    // Keep the attributes and the doc comment right above it.
    let mut start = at;
    while let Some(prev) = start.checked_sub(1).map(|i| lines[i].trim()) {
        if prev.starts_with("@[") {
            start -= 1;
        } else if prev.ends_with("-/") {
            // Up to the line that opens the doc comment.
            let mut open = start - 1;
            while open > 0 && !lines[open].contains("/--") {
                open -= 1;
            }
            if !lines[open].contains("/--") {
                break;
            }
            start = open;
        } else {
            break;
        }
    }
    let mut end = at + 1;
    while end < lines.len() && !starts_decl(lines[end]) {
        end += 1;
    }
    let block = lines[start..end].join("\n");
    Some(block.trim_end().to_string())
}

/// The hunks of a unified diff that mention `name`, with their file headers;
/// `None` if none does.
pub fn diff_hunks_mentioning(diff: &str, name: &str) -> Option<String> {
    let mut out = String::new();
    let mut header = String::new();
    let mut hunk = String::new();
    let mut header_shown = false;
    let flush = |hunk: &mut String, header: &str, header_shown: &mut bool, out: &mut String| {
        if !hunk.is_empty() && mentions(hunk, name) {
            if !*header_shown {
                out.push_str(header);
                *header_shown = true;
            }
            out.push_str(hunk);
        }
        hunk.clear();
    };
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git") {
            flush(&mut hunk, &header, &mut header_shown, &mut out);
            header = line.to_string();
            header_shown = false;
        } else if line.starts_with("@@") {
            flush(&mut hunk, &header, &mut header_shown, &mut out);
            hunk.push_str(line);
        } else if hunk.is_empty() {
            header.push_str(line);
        } else {
            hunk.push_str(line);
        }
    }
    flush(&mut hunk, &header, &mut header_shown, &mut out);
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cell_types_round_trip_through_the_database() {
        for t in CellType::ALL {
            assert_eq!(CellType::from_db(t.kind(), t.variant()), Some(t), "{t:?}");
        }
        assert_eq!(CellType::from_db("sink", Some("scheduled")), None);
        assert!(!CellType::UiInput.has_function() && CellType::UiInput.has_input());
        assert!(CellType::Scheduled.has_function() && !CellType::Scheduled.in_graph());
        assert!(CellType::ChannelSink.in_graph() && !CellType::ChannelSink.has_input());
    }

    #[test]
    fn cells_are_validated() {
        let c = |f: fn(&mut CellConfig)| {
            let mut c = CellConfig::default();
            f(&mut c);
            c
        };
        let (name, _, cfg) = validate_cell(
            CellType::Scheduled,
            "prices",
            "fetch the prices",
            &c(|c| {
                c.url = Some(" https://example.com/prices ".into());
                c.schedule = Some("0 * * * *".into());
            }),
        )
        .unwrap();
        assert_eq!(name, "prices");
        assert_eq!(cfg.input.as_deref(), Some("prices"));
        assert_eq!(cfg.url.as_deref(), Some("https://example.com/prices"));
        // Refusals: bad name, private URL, bad cron, missing interface.
        assert!(validate_cell(CellType::Node, "Total", "x", &CellConfig::default()).is_err());
        assert!(validate_cell(CellType::Node, "total", " ", &CellConfig::default()).is_err());
        assert!(validate_cell(
            CellType::HttpSink,
            "alert",
            "x",
            &c(|c| c.url = Some("http://10.0.0.1/hook".into()))
        )
        .is_err());
        assert!(validate_cell(
            CellType::Watch,
            "w",
            "x",
            &c(|c| {
                c.url = Some("https://example.com".into());
                c.schedule = Some("every hour".into());
            })
        )
        .is_err());
        assert!(
            validate_cell(CellType::ChannelSink, "reply", "x", &CellConfig::default()).is_err()
        );
        let (_, _, ui) = validate_cell(
            CellType::UiInput,
            "country",
            "pick",
            &c(|c| c.choices = vec![" FR".into(), "DE".into(), "FR".into(), "".into()]),
        )
        .unwrap();
        assert_eq!(ui.choices, vec!["FR", "DE"]);
        let (_, _, sink) =
            validate_cell(CellType::UiSink, "view", "show", &CellConfig::default()).unwrap();
        assert_eq!(sink.format.as_deref(), Some("json"));
        assert!(validate_cell(
            CellType::StorageSink,
            "archive",
            "x",
            &c(|c| {
                c.connection_id = Some("abc".into());
                c.path = Some("reports/../x".into());
            })
        )
        .is_err());
    }

    #[test]
    fn the_ssrf_guard_refuses_private_space() {
        for bad in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:10.0.0.1",
            "224.0.0.1",
        ] {
            assert!(!is_public_ip(bad.parse().unwrap()), "{bad}");
        }
        for good in ["93.184.216.34", "2606:4700:4700::1111", "8.8.8.8"] {
            assert!(is_public_ip(good.parse().unwrap()), "{good}");
        }
        assert!(validate_fetch_url("https://example.com/a?b=1").is_ok());
        assert!(validate_fetch_url("http://[::1]:8080/").is_err());
        assert!(validate_fetch_url("https://user:pw@example.com/").is_err());
        assert!(validate_fetch_url("file:///etc/passwd").is_err());
        assert!(validate_fetch_url("https://localhost/").is_err());
        assert!(validate_fetch_url("https://metadata.google.internal/").is_err());
        assert_eq!(host_of("example.com:8443"), "example.com");
        assert_eq!(host_of("[2001:db8::1]:80"), "2001:db8::1");
    }

    /// 2026-09-29 12:34:56 UTC (a Tuesday).
    const T: i64 = 1_790_685_296;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(T.div_euclid(86_400)), (2026, 9, 29));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn cron_due_times() {
        let next = |e: &str| Cron::parse(e).unwrap().next_after(T).unwrap();
        // Every minute: the next whole minute.
        assert_eq!(next("* * * * *"), 1_790_685_300);
        // Hourly on the hour: 13:00.
        assert_eq!(next("0 * * * *"), 1_790_686_800);
        assert_eq!(next("@hourly"), 1_790_686_800);
        // Weekdays at 9: Wednesday 2026-09-30 09:00.
        assert_eq!(next("0 9 * * 1-5"), 1_790_758_800);
        // Sundays (0 and 7) at midnight: 2026-10-04.
        assert_eq!(next("0 0 * * 0"), 1_791_072_000);
        assert_eq!(next("0 0 * * 7"), 1_791_072_000);
        // Every 15 minutes: 12:45.
        assert_eq!(next("*/15 * * * *"), 1_790_685_900);
        // The first of the month: 2026-10-01.
        assert_eq!(next("0 0 1 * *"), 1_790_812_800);
        // Either day field when both are restricted: the 1st or a Wednesday → 09-30.
        assert_eq!(next("0 0 1 * 3"), 1_790_726_400);
        // Leap day.
        let leap = Cron::parse("0 0 29 2 *").unwrap().next_after(T).unwrap();
        assert_eq!(civil_from_days(leap / 86_400), (2028, 2, 29));
        assert_eq!(Cron::parse("0 0 30 2 *").unwrap().next_after(T), None);
        for bad in [
            "",
            "* * * *",
            "60 * * * *",
            "* 24 * * *",
            "5-1 * * * *",
            "*/0 * * * *",
            "a * * * *",
        ] {
            assert!(Cron::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn reads_lun_json() {
        let lun: LunJson = serde_json::from_value(json!({
            "open": ["Sheet"],
            "functions": [
                {"name": "prices", "module": "Sheet.Prices", "function": "Sheet.prices",
                 "signature": "String → Eff [HTTP.HTTP cap, Error String] (List Float)"},
                {"name": "total", "module": "Sheet", "function": "Sheet.total",
                 "signature": "List Float → Eff [] Float"}
            ],
            "graphs": [{"name": "main", "program":
                "do\n  let ps ← input \"prices\" (List Float)\n  let c ← input \"country\" String -- picked\n  total ps"}]
        }))
        .unwrap();
        assert_eq!(
            program_inputs(&lun.graphs[0].program),
            vec![
                ("prices".to_string(), "(List Float)".to_string()),
                ("country".to_string(), "String".to_string())
            ]
        );
        let cfg = CellConfig {
            input: Some("prices".into()),
            ..Default::default()
        };
        let imp = cell_impl(CellType::Scheduled, "prices", &cfg, &lun);
        assert_eq!(imp.function.as_deref(), Some("prices"));
        assert_eq!(imp.effects, vec!["HTTP", "Error"]);
        assert_eq!(imp.input_type.as_deref(), Some("(List Float)"));
        let ui = cell_impl(
            CellType::UiInput,
            "country",
            &CellConfig {
                input: Some("country".into()),
                ..Default::default()
            },
            &lun,
        );
        assert_eq!(ui.function, None);
        assert_eq!(ui.input_type.as_deref(), Some("String"));
        assert_eq!(signature_effects("Nat → Eff [] Nat"), Vec::<String>::new());
        assert_eq!(signature_effects("Nat → Nat"), Vec::<String>::new());
    }

    #[test]
    fn reads_nodes_and_their_closure() {
        let nodes = parse_nodes(Some(&json!([
            {"id": 0, "input": "x", "output": 5},
            {"id": 1, "function": "double", "args": [0], "output": 10},
            {"id": 2, "function": "add", "args": [1, 0], "error": "boom"},
            {"id": 3, "function": "render", "args": [2], "skipped": 2},
            {"id": 4, "input": "y"}
        ])));
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[1].outcome.as_ref().unwrap().output, Some(json!(10)));
        assert_eq!(
            nodes[2].outcome.as_ref().unwrap().error.as_deref(),
            Some("boom")
        );
        assert_eq!(nodes[3].outcome.as_ref().unwrap().skipped, Some(2));
        assert_eq!(nodes[4].outcome, None);
        assert_eq!(dependencies(&nodes, 3), (vec![2], vec![0, 1, 2]));
        assert_eq!(dependencies(&nodes, 0), (vec![], vec![]));
        let cell = Cell {
            id: "c".into(),
            position: 0,
            name: "render".into(),
            cell_type: CellType::UiSink,
            description: "d".into(),
            config: CellConfig::default(),
            implementation: Some(CellImpl {
                function: Some("render".into()),
                ..Default::default()
            }),
            secret_set: false,
            has_endpoint: false,
            last_input: None,
            next_due_at: None,
            last_check: None,
            writing: false,
            issue: None,
            stale: false,
        };
        assert_eq!(node_of(&nodes, &cell).map(|n| n.id), Some(3));
        assert_eq!(node_label(&nodes[0]), "#0 input x");
    }

    #[test]
    fn widgets_and_types() {
        assert_eq!(
            widget_for(Some("Nat"), &[]),
            Widget::Number {
                integer: true,
                natural: true
            }
        );
        assert_eq!(widget_for(Some("String"), &[]), Widget::Text);
        assert_eq!(widget_for(Some("Bool"), &[]), Widget::Toggle);
        assert_eq!(
            widget_for(None, &["a".into()]),
            Widget::Select(vec!["a".into()])
        );
        assert_eq!(widget_for(Some("(List Nat)"), &[]), Widget::Json);
        assert!(json_fits(&json!(3), Some("Nat")).is_ok());
        assert!(json_fits(&json!(-3), Some("Nat")).is_err());
        assert!(json_fits(&json!("x"), Some("Float")).is_err());
        assert!(json_fits(&json!([1]), Some("List Nat")).is_ok());
        assert!(json_fits(&json!({"a": 1}), Some("Order")).is_ok());
    }

    #[test]
    fn the_message_to_lode_names_every_cell() {
        let cell = |name: &str, cell_type, config| Cell {
            id: name.into(),
            position: 0,
            name: name.into(),
            cell_type,
            description: "does things\nwell".into(),
            config,
            implementation: None,
            secret_set: false,
            has_endpoint: false,
            last_input: None,
            next_due_at: None,
            last_check: None,
            writing: false,
            issue: None,
            stale: false,
        };
        let message = lode_message(
            "Invoices",
            &[
                cell(
                    "country",
                    CellType::UiInput,
                    CellConfig {
                        input: Some("country".into()),
                        ..Default::default()
                    },
                ),
                cell(
                    "api_key",
                    CellType::Secret,
                    CellConfig {
                        name: Some("pricing".into()),
                        ..Default::default()
                    },
                ),
                cell("total", CellType::Node, CellConfig::default()),
            ],
            Some("use cents"),
        );
        assert!(message.contains("named `main`"));
        assert!(message.contains("1. `country`"));
        assert!(message.contains("input \"country\""));
        assert!(message.contains("secret `pricing`"));
        assert!(message.contains("3. `total`"));
        assert!(message.contains("   well"));
        assert!(message.ends_with("From the user: use cents\n"));
    }
    #[test]
    fn words_are_mentioned_whole() {
        assert!(mentions("def total (xs : List Nat)", "total"));
        assert!(mentions("`total`: error", "total"));
        assert!(!mentions("subtotal xs", "total"));
        assert!(!mentions("total_price", "total"));
        assert!(mentions("Sheet.total", "total"));
        assert!(!mentions("anything", ""));
    }

    fn a_cell(name: &str, cell_type: CellType) -> Cell {
        Cell {
            id: name.into(),
            position: 0,
            name: name.into(),
            cell_type,
            description: "Sum the prices.".into(),
            config: CellConfig::default(),
            implementation: None,
            secret_set: false,
            has_endpoint: false,
            last_input: None,
            next_due_at: None,
            last_check: None,
            writing: false,
            issue: None,
            stale: false,
        }
    }

    #[test]
    fn phases() {
        let mut c = a_cell("total", CellType::Node);
        assert_eq!(cell_phase(&c, false, None), CellPhase::NoCode);
        c.writing = true;
        assert_eq!(cell_phase(&c, true, None), CellPhase::Writing);
        // A run that is over does not leave the cell "writing".
        assert_eq!(cell_phase(&c, false, None), CellPhase::NoCode);
        c.writing = false;
        c.implementation = Some(CellImpl {
            function: Some("total".into()),
            ..Default::default()
        });
        assert_eq!(cell_phase(&c, false, None), CellPhase::Running);
        let failed = GraphNode {
            id: 1,
            outcome: Some(NodeOutcome {
                error: Some("boom".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(cell_phase(&c, false, Some(&failed)), CellPhase::Failed);
        c.stale = true;
        assert_eq!(cell_phase(&c, false, None), CellPhase::Stale);
        // An input cell's code is its declared input.
        let mut ui = a_cell("country", CellType::UiInput);
        ui.implementation = Some(CellImpl::default());
        assert_eq!(cell_phase(&ui, true, None), CellPhase::Running);
    }

    #[test]
    fn repair_messages() {
        let mut c = a_cell("total", CellType::Node);
        c.implementation = Some(CellImpl {
            function: Some("total".into()),
            signature: Some("List Nat → Eff [] Nat".into()),
            module: Some("Sheet".into()),
            ..Default::default()
        });
        let m = repair_message("Invoices", &c, "it forgets the shipping");
        assert!(m.contains("the cell `total` (Node)"));
        assert!(m.contains("Currently: `total : List Nat → Eff [] Nat` in `Sheet`."));
        assert!(m.contains("What is wrong: it forgets the shipping"));
        let b = build_repair_message("Invoices", &[&c], "function total: type mismatch");
        assert!(b.contains("function total: type mismatch") && b.contains("- `total` (Node)"));
    }

    #[test]
    fn module_files() {
        assert_eq!(
            module_file("Sheet.Prices").as_deref(),
            Some("Sheet/Prices.lean")
        );
        assert_eq!(module_file("Sheet").as_deref(), Some("Sheet.lean"));
        assert_eq!(module_file("Sheet..X"), None);
        assert_eq!(module_file("../etc"), None);
        assert_eq!(module_file("A.1b"), None);
    }

    #[test]
    fn definitions_are_extracted() {
        let src = "import Linen\n\nnamespace Sheet\n\n/-- The total,\n  in cents. -/\n@[inline]\ndef total (xs : List Nat) : Eff [] Nat := do\n  let s := xs.foldl (· + ·) 0\n\n  pure s\n\ndef subtotal : Nat := 1\n\nend Sheet\n";
        assert_eq!(
            extract_definition(src, "Sheet.total").unwrap(),
            "/-- The total,\n  in cents. -/\n@[inline]\ndef total (xs : List Nat) : Eff [] Nat := do\n  let s := xs.foldl (· + ·) 0\n\n  pure s"
        );
        assert_eq!(
            extract_definition(src, "subtotal").unwrap(),
            "def subtotal : Nat := 1"
        );
        assert_eq!(extract_definition(src, "missing"), None);
        assert!(extract_definition("partial def Sheet.loop : IO Unit := loop", "loop").is_some());
    }

    #[test]
    fn diff_hunks() {
        let diff = "diff --git a/S.lean b/S.lean\n--- a/S.lean\n+++ b/S.lean\n@@ -1,2 +1,2 @@\n-def total := 1\n+def total := 2\n@@ -9 +9 @@\n-def other := 1\n+def other := 3\ndiff --git a/lun.json b/lun.json\n@@ -1 +1 @@\n-{}\n+{\"name\": \"total\"}\n";
        let h = diff_hunks_mentioning(diff, "total").unwrap();
        assert!(h.contains("+def total := 2") && h.contains("lun.json"));
        assert!(!h.contains("other"));
        assert!(h.starts_with("diff --git a/S.lean"));
        assert_eq!(diff_hunks_mentioning(diff, "absent"), None);
    }
}
