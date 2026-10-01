//! Notebooks (docs/computations.md §1.2): the cells in order — each a
//! description, its config, the implementation `lode` produced and the
//! node's last outcome — with "implement" and "steer" against lode, inputs
//! fed to the running graph, and the graph's sources, sinks and
//! dependencies shown per cell.

use api::{
    abort_graph, add_cell, cell_code, cell_phase, current_user, delete_cell, delete_graph,
    dependencies, describe_config, feed_cell, get_graph, graph_progress, implement_graph,
    list_connections, list_graphs, mentions, move_cell, node_label, node_of, rebuild_graph,
    report_cell, restart_graph, rotate_endpoint, set_cell_secret, set_graph_model, update_cell,
    validate_cell, widget_for, Cell, CellConfig, CellPhase, CellType, Channel, Connection,
    FeedResult, Graph, GraphDetail, GraphNode, LodeEntry, Provider, Widget, UI_FORMATS,
    CellOrder, cell_dependencies, ordered_cells, CellConnector, ConnectorPermissions,
    PermissionPreset,
};
use dioxus::prelude::*;
use serde_json::Value;

use crate::auth::LoginPanel;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::input::Input;
use crate::components::label::Label;
use crate::components::select::{Select, SelectOption};
use crate::components::textarea::Textarea;
use crate::connections::{PermissionEditor, CONNECTIONS_CSS};
use crate::orgs::ORGS_CSS;
use crate::render::{pretty, JsonView, Output};
use crate::slug_form::{NewSlugForm, Scope};
use crate::{error_message, navigate_to};

pub(crate) const NOTEBOOK_CSS: Asset = asset!("/assets/styling/notebook.css");

/// Wait `ms` milliseconds in the browser (the notebook's follow loop has no
/// timer of its own; the server's long poll does most of the waiting).
async fn pause(ms: u32) {
    let _ = document::eval(&format!(
        "await new Promise(r => setTimeout(r, {ms})); return true;"
    ))
    .await;
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

/// `Anthropic`, or `OpenAI-compatible · api.example.com`.
fn connection_name(c: &Connection) -> String {
    if c.label == c.provider.name() {
        c.label.clone()
    } else {
        format!("{} · {}", c.provider.name(), c.label)
    }
}

fn status_class(status: &str) -> &'static str {
    match status {
        "ready" => "conn-status conn-status-active",
        "implementing" => "conn-status conn-status-pending",
        "failed" => "conn-status conn-status-failed",
        _ => "conn-status",
    }
}

// ── The project's notebooks ─────────────────────────────────────────────

#[component]
pub(crate) fn GraphsPanel(slug: ReadSignal<String>, project: ReadSignal<String>) -> Element {
    let mut graphs = use_server_future(move || list_graphs(slug(), project()))?;
    rsx! {
        Card {
            CardHeader {
                CardTitle { "Notebooks" }
                CardDescription {
                    "Cells in plain language, implemented as typed Lean functions in the primary "
                    "repository, and run as a reactive graph."
                }
            }
            CardContent {
                match graphs() {
                    None => rsx! { p { "Loading…" } },
                    Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load notebooks: {error_message(&e)}" } },
                    Some(Ok(list)) if list.is_empty() => rsx! { p { class: "orgs-empty", "No notebook yet — create one below." } },
                    Some(Ok(list)) => rsx! {
                        table { class: "orgs-table",
                            thead { tr { th { "Slug" } th { "Name" } th { "Status" } th { "Updated" } } }
                            tbody {
                                for g in list {
                                    tr { key: "{g.id}",
                                        td { Link { to: "/orgs/{slug}/projects/{project}/graphs/{g.slug}", code { "{g.slug}" } } }
                                        td { "{g.name}" }
                                        td { span { class: status_class(&g.status), "{g.status}" } }
                                        td { "{g.updated_at}" }
                                    }
                                }
                            }
                        }
                    },
                }
                h4 { class: "projects-new", "New notebook" }
                NewSlugForm {
                    scope: Scope::Graph { org: slug(), project: project() },
                    name_placeholder: "Invoices",
                    slug_placeholder: "invoices",
                    on_created: move |created: String| {
                        graphs.restart();
                        navigate_to(&format!("/orgs/{}/projects/{}/graphs/{created}", slug(), project()));
                    },
                }
            }
        }
    }
}

// ── The notebook page ───────────────────────────────────────────────────

#[component]
pub fn NotebookPage(
    slug: ReadSignal<String>,
    project: ReadSignal<String>,
    graph: ReadSignal<String>,
) -> Element {
    let me = use_server_future(current_user)?;
    let mut detail = use_server_future(move || get_graph(slug(), project(), graph()))?;
    let connections = use_server_future(move || list_connections(slug()))?;

    // lode's log and the notebook's live status, followed while it is
    // implementing: the server holds each request until lode has news.
    let mut log = use_signal(Vec::<LodeEntry>::new);
    let mut next = use_signal(|| 0u64);
    let mut live = use_signal(|| None::<Graph>);
    // The nodes of the last feed, over the page's (keyed by the page's
    // `updated_at`, so a reload supersedes them): feeding an input updates
    // the outputs in place instead of reloading the page.
    let mut fed = use_signal(|| None::<(String, Vec<GraphNode>)>);
    let mut follow = use_future(move || async move {
        // Set when "Implement" (re)started the loop: the page is reloaded
        // once the run is over, even if it was over by the first answer.
        let mut was_implementing = live
            .peek()
            .as_ref()
            .is_some_and(|g| g.status == "implementing");
        loop {
            match graph_progress(slug(), project(), graph(), next(), 20).await {
                Ok(p) => {
                    let from = next();
                    log.write()
                        .extend(p.entries.into_iter().filter(|e| e.index >= from));
                    next.set(p.next);
                    let implementing = p.graph.status == "implementing";
                    live.set(Some(p.graph));
                    if !implementing {
                        if was_implementing {
                            detail.restart();
                        }
                        break;
                    }
                    was_implementing = true;
                    // lode holds the request while it has nothing new; a
                    // floor all the same, and a longer one while lun builds.
                    pause(if p.state == "running" { 1000 } else { 4000 }).await;
                }
                Err(_) => pause(10_000).await,
            }
        }
    });

    // A run was launched (implement, a report, an automatic rewrite): follow
    // it, and reload the page for the cells now being written.
    let started = use_callback(move |g: Graph| {
        fed.set(None);
        live.set(Some(g));
        follow.restart();
        spawn(async move {
            // After the event that launched it has settled.
            pause(50).await;
            detail.restart();
        });
    });

    if let Some(Ok(None)) = me() {
        return rsx! { LoginPanel { error: None } };
    }
    let ai: Vec<Connection> = match connections() {
        Some(Ok(list)) => list.into_iter().filter(|c| c.provider.is_ai()).collect(),
        _ => Vec::new(),
    };
    let storage: Vec<Connection> = match connections() {
        Some(Ok(list)) => list
            .into_iter()
            .filter(|c| Provider::STORAGE.contains(&c.provider))
            .collect(),
        _ => Vec::new(),
    };
    let available_connections = connections().and_then(Result::ok).unwrap_or_default();

    rsx! {
        document::Link { rel: "stylesheet", href: ORGS_CSS }
        document::Link { rel: "stylesheet", href: CONNECTIONS_CSS }
        document::Link { rel: "stylesheet", href: NOTEBOOK_CSS }
        div { class: "orgs nb",
            p { class: "back-link", Link { to: "/orgs/{slug}/projects/{project}", "← Project" } }
            match detail() {
                None => rsx! { p { "Loading…" } },
                Some(Err(e)) => rsx! { p { class: "orgs-error", "Could not load this notebook: {error_message(&e)}" } },
                Some(Ok(d)) => {
                    let current = live().filter(|g| g.id == d.graph.id && g.updated_at >= d.graph.updated_at).unwrap_or(d.graph.clone());
                    rsx! {
                        Header {
                            slug: slug(),
                            project: project(),
                            detail: d.clone(),
                            graph: current.clone(),
                            ai: ai.clone(),
                            on_started: started,
                            on_changed: move |_| detail.restart(),
                        }
                        if !log().is_empty() {
                            LodeLog { entries: log(), running: current.status == "implementing" }
                        }
                        Cells {
                            slug: slug(),
                            project: project(),
                            nodes: fed().filter(|(at, _)| *at == d.graph.updated_at).map(|(_, n)| n).unwrap_or(d.nodes.clone()),
                            detail: d.clone(),
                            graph: current.clone(),
                            log: log(),
                            ready: current.session,
                            storage: storage.clone(),
                            connections: available_connections.clone(),
                            on_changed: move |_| detail.restart(),
                            on_started: started,
                            on_fed: {
                                let at = d.graph.updated_at.clone();
                                move |r: FeedResult| {
                                    fed.set(Some((at.clone(), r.nodes)));
                                    // An error lun reported sent a cell back to lode.
                                    if let Some(g) = r.repair {
                                        started.call(g);
                                    }
                                }
                            },
                        }
                        ActivityCard { detail: d.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn Header(
    slug: String,
    project: String,
    detail: GraphDetail,
    graph: Graph,
    ai: Vec<Connection>,
    on_started: EventHandler<Graph>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut note = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut confirming = use_signal(|| false);
    let implementing = graph.status == "implementing";
    let g = detail.graph.slug.clone();

    let run = {
        let (slug, project, g) = (slug.clone(), project.clone(), g.clone());
        move |steer: bool| {
            let (slug, project, g) = (slug.clone(), project.clone(), g.clone());
            async move {
                busy.set(true);
                match implement_graph(slug, project, g, note(), steer).await {
                    Ok(graph) => {
                        error.set(None);
                        note.set(String::new());
                        on_started.call(graph);
                    }
                    Err(e) => error.set(Some(error_message(&e))),
                }
                busy.set(false);
            }
        }
    };
    let implement = run.clone();
    let steer = run;
    let act = {
        let (slug, project, g) = (slug.clone(), project.clone(), g.clone());
        move |what: &'static str| {
            let (slug, project, g) = (slug.clone(), project.clone(), g.clone());
            async move {
                busy.set(true);
                let result = match what {
                    "abort" => abort_graph(slug, project, g).await.map(|_| ()),
                    "rebuild" => rebuild_graph(slug, project, g)
                        .await
                        .map(|graph| on_started.call(graph)),
                    _ => restart_graph(slug, project, g).await.map(|_| ()),
                };
                match result {
                    Ok(()) => {
                        error.set(None);
                        on_changed.call(());
                    }
                    Err(e) => error.set(Some(error_message(&e))),
                }
                busy.set(false);
            }
        }
    };
    let (abort, rebuild, restart) = (act.clone(), act.clone(), act);
    let delete = {
        let (slug, project, g) = (slug.clone(), project.clone(), g.clone());
        move |_| {
            let (slug, project, g) = (slug.clone(), project.clone(), g.clone());
            async move {
                if !confirming() {
                    confirming.set(true);
                    return;
                }
                match delete_graph(slug.clone(), project.clone(), g).await {
                    Ok(()) => navigate_to(&format!("/orgs/{slug}/projects/{project}")),
                    Err(e) => {
                        error.set(Some(error_message(&e)));
                        confirming.set(false);
                    }
                }
            }
        }
    };

    rsx! {
        Card {
            CardHeader {
                CardTitle { "{detail.graph.name}" }
                CardDescription {
                    code { "{detail.org.slug}/{detail.project.slug}/{detail.graph.slug}" }
                    " · "
                    span { class: status_class(&graph.status), "{graph.status}" }
                    if let Some(commit) = graph.commit.clone() {
                        " · commit "
                        match detail.project.repo.as_ref().filter(|r| r.provider == Provider::Github) {
                            Some(repo) => rsx! { a { href: "{repo.web_url}/commit/{commit}", target: "_blank", rel: "noopener", code { "{short(&commit)}" } } },
                            None => rsx! { code { "{short(&commit)}" } },
                        }
                    }
                    if graph.session { " · session live" }
                }
            }
            CardContent {
                if let Some(d) = graph.detail.clone() {
                    p { class: if graph.status == "failed" { "nb-detail orgs-error" } else { "nb-detail" }, "{d}" }
                }
                if detail.project.repo.is_none() {
                    p { class: "orgs-error",
                        "Set the project's primary repository first: the notebook's code is written there. "
                        Link { to: "/orgs/{slug}/projects/{project}/settings/repository", "Choose one in the project's settings" }
                    }
                }
                ModelPicker {
                    slug: slug.clone(),
                    project: project.clone(),
                    graph: detail.graph.clone(),
                    ai,
                    on_changed: move |_| on_changed.call(()),
                }
                div { class: "conn-subform",
                    div { class: "orgs-field conn-wide",
                        Label { html_for: "nb-note",
                            if implementing { "Steer the writing (taken into account at the next step)" } else { "A note for the code writing (optional)" }
                        }
                        Textarea {
                            id: "nb-note",
                            rows: 2,
                            placeholder: if implementing { "Use cents, not floats." } else { "Anything to know beyond the cells." },
                            value: note(),
                            oninput: move |e: FormEvent| note.set(e.value()),
                        }
                    }
                    div { class: "conn-actions",
                        if implementing {
                            Button { disabled: busy() || note().trim().is_empty(), onclick: move |_| steer(true), "Steer" }
                            Button { variant: ButtonVariant::Outline, disabled: busy(), onclick: move |_| abort("abort"), "Abort" }
                        } else {
                            Button {
                                disabled: busy() || detail.cells.is_empty() || detail.project.repo.is_none(),
                                onclick: move |_| implement(false),
                                 if graph.commit.is_some() { "Regenerate all code" } else { "Generate all code" }
                            }
                            Button {
                                variant: ButtonVariant::Outline,
                                disabled: busy() || detail.project.repo.is_none(),
                                onclick: move |_| rebuild("rebuild"),
                                "Rebuild from the repository"
                            }
                            if graph.status == "ready" {
                                Button { variant: ButtonVariant::Outline, disabled: busy(), onclick: move |_| restart("restart"), "Restart session" }
                            }
                        }
                        Button {
                            size: ButtonSize::Sm,
                            variant: if confirming() { ButtonVariant::Destructive } else { ButtonVariant::Ghost },
                            onclick: delete,
                            if confirming() { "Confirm: delete the notebook, its cells and secrets" } else { "Delete notebook" }
                        }
                    }
                }
                if let Some(e) = error() {
                    p { class: "orgs-error", "{e}" }
                }
            }
        }
    }
}

/// The AI connection and model lode works with.
#[component]
fn ModelPicker(
    slug: String,
    project: String,
    graph: Graph,
    ai: Vec<Connection>,
    on_changed: EventHandler<()>,
) -> Element {
    let ai: Vec<Connection> = ai
        .into_iter()
        .filter(|c| c.provider.can_generate())
        .collect();
    let mut connection = use_signal(|| {
        graph
            .model_connection_id
            .clone()
            .or_else(|| ai.first().map(|c| c.id.clone()))
            .unwrap_or_default()
    });
    let mut model = use_signal(|| graph.model_name.clone().unwrap_or_default());
    let ai_for_price = ai.clone();
    let price_provider = use_memo(move || {
        ai_for_price
            .iter()
            .find(|c| c.id == connection())
            .map(|c| c.provider)
            .unwrap_or(Provider::Anthropic)
    });
    let mut status = use_signal(|| None::<Result<String, String>>);
    if ai.is_empty() {
        return rsx! {
            p { class: "conn-meta orgs-error",
                "Connect an AI provider in the organisation first: the code is written with its models. "
                Link { to: "/orgs/{slug}/settings/connections", "Connect one in the organisation's settings" }
            }
        };
    }
    let model_slug = slug.clone();
    let save = move |_| {
        let (slug, project, g) = (slug.clone(), project.clone(), graph.slug.clone());
        async move {
            match set_graph_model(slug, project, g, connection(), model()).await {
                Ok(_) => {
                    status.set(Some(Ok("Saved.".to_string())));
                    on_changed.call(());
                }
                Err(e) => status.set(Some(Err(error_message(&e)))),
            }
        }
    };
    rsx! {
        div { class: "conn-picker",
            div { class: "orgs-field conn-select",
                Label { html_for: "nb-model-conn", "Model through" }
                Select::<String> {
                    default_value: connection(),
                    aria_label: "AI connection",
                    on_value_change: move |v: Option<String>| {
                        if let Some(id) = v {
                            connection.set(id);
                            model.set(String::new());
                        }
                    },
                    for (i, c) in ai.iter().enumerate() {
                        SelectOption::<String> {
                            key: "{c.id}",
                            index: i,
                            value: c.id.clone(),
                            text_value: connection_name(c),
                            "{connection_name(c)}"
                        }
                    }
                }
            }
            div { class: "orgs-field",
                crate::ai_models::ModelPicker { key: "{connection}", slug: model_slug.clone(), connection_id: connection(), value: model, disabled: graph.status == "implementing" }
            }
            Button { variant: ButtonVariant::Outline, disabled: model().trim().is_empty() || graph.status == "implementing", onclick: save, "Save model" }
        }
        crate::ai_cost::ModelCost { provider: price_provider(), model: model() }
        match status() {
            Some(Ok(m)) => rsx! { p { class: "conn-result ok", "{m}" } },
            Some(Err(e)) => rsx! { p { class: "orgs-error", "{e}" } },
            None => rsx! {},
        }
    }
}

#[component]
fn LodeLog(entries: Vec<LodeEntry>, running: bool) -> Element {
    rsx! {
        Card {
            CardHeader {
                CardTitle { "Implementation" }
                CardDescription {
                    if running { "Writing the code; this follows each step." } else { "The last time the code was written." }
                }
            }
            CardContent {
                div { class: "nb-log",
                    for e in entries.iter().rev().take(200) {
                        div { key: "{e.index}", class: "nb-log-entry nb-log-{e.kind}",
                            span { class: "nb-log-kind", "{e.kind}" }
                            span { class: "nb-log-text", "{e.text}" }
                            if let Some(usage) = e.usage.as_ref() {
                                span { class: "conn-meta", " · tokens: {usage.input} input, {usage.output} output, {usage.cache_read} cached read, {usage.cache_write} cached write" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Cells ───────────────────────────────────────────────────────────────

#[component]
fn Cells(
    slug: String,
    project: String,
    detail: GraphDetail,
    nodes: Vec<GraphNode>,
    graph: Graph,
    log: Vec<LodeEntry>,
    ready: bool,
    storage: Vec<Connection>,
    connections: Vec<Connection>,
    on_changed: EventHandler<()>,
    on_started: EventHandler<Graph>,
    on_fed: EventHandler<FeedResult>,
) -> Element {
    let mut order = use_signal(|| CellOrder::Declaration);
    let mut diagram = use_signal(|| false);
    let mut selected = use_signal(|| None::<String>);
    let implementing = graph.status == "implementing";
    // This run's part of lode's log: what each cell's "follow the writing" shows.
    let run: Vec<LodeEntry> = log
        .into_iter()
        .filter(|e| e.index >= graph.log_start)
        .collect();
    let count = detail.cells.len();
    let ordered = ordered_cells(&detail.cells, &nodes, order());
    rsx! {
        Card {
            CardHeader {
                CardTitle { "Cells" }
                CardDescription {
                    "Describe each node in plain language. Sources bring the world in, sinks send results out; "
                    "Typednotes writes their code and runs them as one graph."
                }
            }
            CardContent {
                div { class: "nb-toolbar",
                    div { class: "conn-select",
                        Select::<CellOrder> {
                            default_value: order(), aria_label: "Cell order",
                            on_value_change: move |value: Option<CellOrder>| { if let Some(value) = value { order.set(value); } },
                            SelectOption::<CellOrder> { index: 0usize, value: CellOrder::Declaration, text_value: "Declaration order", "Declaration order" }
                            SelectOption::<CellOrder> { index: 1usize, value: CellOrder::Name, text_value: "Name", "Name" }
                            SelectOption::<CellOrder> { index: 2usize, value: CellOrder::Topological, text_value: "Dependency order", "Dependency order" }
                        }
                    }
                    Button { variant: if !diagram() { ButtonVariant::Secondary } else { ButtonVariant::Ghost }, aria_pressed: (!diagram()).to_string(), onclick: move |_| diagram.set(false), "Notebook" }
                    Button { variant: if diagram() { ButtonVariant::Secondary } else { ButtonVariant::Ghost }, aria_pressed: diagram().to_string(), onclick: move |_| diagram.set(true), "Dependency graph" }
                }
                if detail.cells.is_empty() {
                    p { class: "orgs-empty", "No cell yet — add the first one below." }
                }
                if diagram() && !detail.cells.is_empty() {
                    DependencyGraph { cells: detail.cells.clone(), nodes: nodes.clone(), on_select: move |id: String| selected.set(Some(id)) }
                }
                div { class: "nb-cells",
                    for cell in ordered.iter().filter(|cell| !diagram() || selected().as_deref() == Some(cell.id.as_str())) {
                        CellView {
                            key: "{cell.id}",
                            slug: slug.clone(),
                            project: project.clone(),
                            graph: detail.graph.slug.clone(),
                            cell: cell.clone(),
                            index: detail.cells.iter().position(|c| c.id == cell.id).unwrap_or(0),
                            declaration: cell.position + 1,
                            count,
                            cells: detail.cells.clone(),
                            reorder: order() == CellOrder::Declaration,
                            nodes: nodes.clone(),
                            sources: detail.sources.clone(),
                            sinks: detail.sinks.clone(),
                            channels: detail.channels.clone(),
                            storage: storage.clone(),
                            connections: connections.clone(),
                            ready,
                            implementing,
                            run: run.clone(),
                            on_changed: move |_| on_changed.call(()),
                            on_started: move |g: Graph| on_started.call(g),
                            on_fed: move |r: FeedResult| on_fed.call(r),
                        }
                    }
                }
                CellEditor {
                    slug: slug.clone(),
                    project: project.clone(),
                    graph: detail.graph.slug.clone(),
                    existing: None,
                    cells: detail.cells.clone(),
                    nodes: nodes.clone(),
                    channels: detail.channels.clone(),
                    storage: storage.clone(),
                    connections: connections.clone(),
                    on_saved: move |_| on_changed.call(()),
                    on_cancel: None,
                }
            }
        }
    }
}

/// One lane per dependency depth. Every node links to its editable notebook cell.
#[component]
fn DependencyGraph(cells: Vec<Cell>, nodes: Vec<GraphNode>, on_select: EventHandler<String>) -> Element {
    let sorted = ordered_cells(&cells, &nodes, CellOrder::Topological);
    let mut levels = std::collections::BTreeMap::<String, usize>::new();
    let mut rows = std::collections::BTreeMap::<usize, usize>::new();
    let mut positions = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for cell in &sorted {
        let depth = cell_dependencies(cell, &cells, &nodes).iter().filter_map(|name| levels.get(name)).max().map_or(0, |n| n + 1);
        let row = rows.entry(depth).or_default();
        positions.insert(cell.name.clone(), (24 + depth * 236, 24 + *row * 100));
        *row += 1;
        levels.insert(cell.name.clone(), depth);
    }
    let width = 48 + (levels.values().copied().max().unwrap_or(0) + 1) * 236;
    let height = 48 + rows.values().copied().max().unwrap_or(1) * 100;
    rsx! {
        div { class: "nb-graph-wrap",
            p { class: "conn-meta", "Arrows run from an input cell to the cells that use it. Select a cell to edit it below." }
            svg { class: "nb-dependency-graph", view_box: "0 0 {width} {height}", width: "{width}", height: "{height}", role: "group", "aria-label": "Notebook cell dependencies",
                defs { marker { id: "nb-arrow", view_box: "0 0 10 10", ref_x: "9", ref_y: "5", marker_width: "6", marker_height: "6", orient: "auto-start-reverse", path { d: "M 0 0 L 10 5 L 0 10 z" } } }
                for cell in &sorted {
                    for name in cell_dependencies(cell, &cells, &nodes) {
                        if let (Some((sx, sy)), Some((tx, ty))) = (positions.get(&name), positions.get(&cell.name)) {
                            path { class: "nb-graph-edge", d: "M {sx + 196} {sy + 34} C {sx + 216} {sy + 34}, {tx - 20} {ty + 34}, {tx} {ty + 34}", marker_end: "url(#nb-arrow)" }
                        }
                    }
                }
                for cell in &sorted {
                    if let Some((x, y)) = positions.get(&cell.name) {
                        g { class: "nb-graph-link", role: "button", tabindex: "0", "aria-label": "Go to cell {cell.name}",
                            onclick: { let id = cell.id.clone(); move |_| on_select.call(id.clone()) },
                            onkeydown: { let id = cell.id.clone(); move |event: KeyboardEvent| {
                                if event.key() == Key::Enter || event.key() == Key::Character(" ".into()) {
                                    event.prevent_default();
                                    on_select.call(id.clone());
                                }
                            } },
                            rect { class: "nb-graph-node nb-graph-{cell.cell_type.kind()}", x: "{x}", y: "{y}", width: "196", height: "68", rx: "8" }
                            text { class: "nb-graph-title", x: "{x + 12}", y: "{y + 27}", "{cell.position + 1}. {cell.name.chars().take(22).collect::<String>()}" }
                            text { class: "nb-graph-kind", x: "{x + 12}", y: "{y + 49}", "{cell.cell_type.label()}" }
                        }
                    }
                }
            }
        }
    }
}

/// How a node's neighbours read: `#1 double, #0 input x`.
fn labels(nodes: &[GraphNode], ids: &[u64]) -> String {
    ids.iter()
        .map(|id| {
            nodes
                .iter()
                .find(|n| n.id == *id)
                .map(node_label)
                .unwrap_or_else(|| format!("#{id}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[component]
fn CellView(
    slug: String,
    project: String,
    graph: String,
    cell: Cell,
    index: usize,
    declaration: i32,
    count: usize,
    cells: Vec<Cell>,
    reorder: bool,
    nodes: Vec<GraphNode>,
    sources: Vec<u64>,
    sinks: Vec<u64>,
    channels: Vec<Channel>,
    storage: Vec<Connection>,
    connections: Vec<Connection>,
    ready: bool,
    implementing: bool,
    run: Vec<LodeEntry>,
    on_changed: EventHandler<()>,
    on_started: EventHandler<Graph>,
    on_fed: EventHandler<FeedResult>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut confirming = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let node = node_of(&nodes, &cell).cloned();
    let phase = cell_phase(&cell, implementing, node.as_ref());
    let config_text = describe_config(cell.cell_type, &cell.config);
    let channel_label = cell
        .config
        .channel_id
        .as_ref()
        .and_then(|id| channels.iter().find(|c| &c.id == id))
        .map(|c| format!("{} {}", c.provider.name(), c.label));

    let mover = {
        let (slug, project, graph, id) = (
            slug.clone(),
            project.clone(),
            graph.clone(),
            cell.id.clone(),
        );
        move |delta: i32| {
            let (slug, project, graph, id) =
                (slug.clone(), project.clone(), graph.clone(), id.clone());
            async move {
                match move_cell(slug, project, graph, id, delta).await {
                    Ok(()) => on_changed.call(()),
                    Err(e) => error.set(Some(error_message(&e))),
                }
            }
        }
    };
    let (up, down) = (mover.clone(), mover);
    let remove = {
        let (slug, project, graph, id) = (
            slug.clone(),
            project.clone(),
            graph.clone(),
            cell.id.clone(),
        );
        move |_| {
            let (slug, project, graph, id) =
                (slug.clone(), project.clone(), graph.clone(), id.clone());
            async move {
                if !confirming() {
                    confirming.set(true);
                    return;
                }
                match delete_cell(slug, project, graph, id).await {
                    Ok(()) => on_changed.call(()),
                    Err(e) => {
                        error.set(Some(error_message(&e)));
                        confirming.set(false);
                    }
                }
            }
        }
    };

    if editing() {
        return rsx! {
            div { class: "nb-cell editing",
                CellEditor {
                    slug,
                    project,
                    graph,
                    existing: Some(cell.clone()),
                    cells: cells.clone(),
                    nodes: nodes.clone(),
                    channels,
                    storage,
                    connections,
                    on_saved: move |_| {
                        editing.set(false);
                        on_changed.call(());
                    },
                    on_cancel: Some(EventHandler::new(move |_| editing.set(false))),
                }
            }
        };
    }

    rsx! {
        div { id: "nb-cell-{cell.id}", class: "nb-cell nb-cell-{cell.cell_type.kind()} nb-phase-is-{phase.id()}",
            div { class: "nb-cell-head",
                span { class: "nb-cell-index", title: "Declaration number", "{declaration}" }
                code { class: "nb-cell-name", "{cell.name}" }
                span { class: "nb-badge nb-badge-{cell.cell_type.kind()}", "{cell.cell_type.label()}" }
                div { class: "nb-cell-actions",
                    if reorder {
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, aria_label: "Move {cell.name} up", disabled: index == 0, onclick: move |_| up(-1), "↑" }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, aria_label: "Move {cell.name} down", disabled: index + 1 >= count, onclick: move |_| down(1), "↓" }
                    }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| editing.set(true), "Edit" }
                    Button {
                        size: ButtonSize::Sm,
                        variant: if confirming() { ButtonVariant::Destructive } else { ButtonVariant::Ghost },
                        onclick: remove,
                        if confirming() { "Confirm delete" } else { "Delete" }
                    }
                }
            }
            p { class: "nb-description", "{cell.description}" }
            div { class: "nb-declarations",
                span { class: "conn-meta", "Inputs" }
                if cell_dependencies(&cell, &cells, &nodes).is_empty() {
                    span { class: "conn-meta", "None (source)" }
                } else {
                    for name in cell_dependencies(&cell, &cells, &nodes) {
                        if let Some(target) = cells.iter().find(|c| c.name == name) {
                            a { class: "nb-reference", href: "#nb-cell-{target.id}", "@{name}" }
                        }
                    }
                }
                if let Some(ty) = &cell.config.output_type {
                    span { class: "conn-meta", "Output constraint" } code { "{ty}" }
                }
            }
            if !config_text.is_empty() {
                p { class: "conn-meta", "{config_text}" }
            }
            if let Some(label) = channel_label {
                p { class: "conn-meta", "interface: {label}" }
            }
            Lifecycle {
                slug: slug.clone(),
                project: project.clone(),
                graph: graph.clone(),
                cell: cell.clone(),
                phase,
                implementing,
                error: node.as_ref().and_then(|n| n.outcome.as_ref()).and_then(|o| o.error.clone()),
                run,
                on_started: move |g: Graph| on_started.call(g),
            }
            ImplView { cell: cell.clone() }
            if let Some(n) = node.clone() {
                Relations { node: n, nodes: nodes.clone(), sources, sinks }
            }
            match cell.cell_type {
                CellType::UiInput => rsx! {
                    InputWidget { key: "{cell.config:?}-{cell.implementation.as_ref().and_then(|i| i.input_type.clone()):?}", slug: slug.clone(), project: project.clone(), graph: graph.clone(), cell: cell.clone(), ready, on_fed: move |r: FeedResult| on_fed.call(r) }
                },
                CellType::Secret => rsx! {
                    SecretField { slug: slug.clone(), project: project.clone(), graph: graph.clone(), cell: cell.clone(), on_set: move |_| on_changed.call(()) }
                },
                CellType::Endpoint => rsx! {
                    EndpointBox { slug: slug.clone(), project: project.clone(), graph: graph.clone(), cell: cell.clone() }
                },
                CellType::Scheduled | CellType::Watch => rsx! {
                    p { class: "conn-meta",
                        match cell.next_due_at.clone() { Some(t) => format!("next check {t}"), None => "not scheduled".to_string() }
                        if let Some(last) = cell.last_check.clone() { " · last: {last}" }
                    }
                },
                _ => rsx! {},
            }
            if cell.cell_type.is_source() && cell.cell_type != CellType::UiInput {
                if let Some(v) = cell.last_input.clone() {
                    details { class: "nb-last",
                        summary { "last value fed" }
                        JsonView { value: v }
                    }
                }
            }
            if let Some(n) = node {
                Outcome { cell: cell.clone(), node: n }
            }
            if let Some(e) = error() {
                p { class: "orgs-error", "{e}" }
            }
        }
    }
}

/// Where the cell is: lode writing its code from the description (with the
/// writing to follow), or lun running it (with the code to read) — and the
/// way back to lode when it fails or does not do the right thing.
#[component]
fn Lifecycle(
    slug: String,
    project: String,
    graph: String,
    cell: Cell,
    phase: CellPhase,
    implementing: bool,
    error: Option<String>,
    run: Vec<LodeEntry>,
    on_started: EventHandler<Graph>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut reporting = use_signal(|| false);
    let mut report = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut failure = use_signal(|| None::<String>);
    let mine: Vec<LodeEntry> = run
        .iter()
        .filter(|e| mentions(&e.text, &cell.name) || mentions(&e.detail, &cell.name))
        .cloned()
        .collect();

    let send = {
        let (slug, project, graph, id) = (
            slug.clone(),
            project.clone(),
            graph.clone(),
            cell.id.clone(),
        );
        move |what: String| {
            let (slug, project, graph, id) =
                (slug.clone(), project.clone(), graph.clone(), id.clone());
            async move {
                busy.set(true);
                match report_cell(slug, project, graph, id, what).await {
                    Ok(g) => {
                        failure.set(None);
                        reporting.set(false);
                        report.set(String::new());
                        open.set(true);
                        on_started.call(g);
                    }
                    Err(e) => failure.set(Some(error_message(&e))),
                }
                busy.set(false);
            }
        }
    };
    let (fix, rewrite, regenerate, submit) = (send.clone(), send.clone(), send.clone(), send);
    let has_code = matches!(
        phase,
        CellPhase::Running | CellPhase::Failed | CellPhase::Stale
    );

    rsx! {
        div { class: "nb-life nb-life-{phase.id()}",
            div { class: "nb-life-head",
                span { class: "nb-phase nb-phase-{phase.id()}",
                    if phase == CellPhase::Writing { span { class: "nb-pulse" } }
                    "{phase.label()}"
                }
                if phase == CellPhase::Writing {
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| open.set(!open()),
                        if open() { "Hide the writing ▴" } else { "Follow the writing ({mine.len()}) ▾" }
                    }
                } else if has_code && cell.cell_type.has_function() {
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| open.set(!open()),
                        if open() { "Hide the code ▴" } else { "Show the code ▾" }
                    }
                }
                if has_code && !reporting() {
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| reporting.set(true),
                        "Not doing the right thing?"
                    }
                }
                if cell.cell_type.has_function() {
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, disabled: busy() || implementing,
                        onclick: move |_| regenerate("Regenerate this cell's code from its current description, dependencies, output type and permissions.".into()),
                        "Regenerate code" }
                }
            }
            if let Some(issue) = cell.issue.clone().filter(|_| phase == CellPhase::Writing) {
                p { class: "nb-issue", "Rewriting because: {issue}" }
            }
            match phase {
                CellPhase::NoCode if !implementing => rsx! {
                    p { class: "conn-meta", "Implement the notebook: its code is written from its description." }
                },
                CellPhase::NoCode => rsx! {
                    p { class: "conn-meta", "Other cells are being written; implement again to include this one." }
                },
                CellPhase::Failed => rsx! {
                    div { class: "conn-actions",
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Outline,
                            disabled: busy(),
                            onclick: {
                                let error = error.clone().unwrap_or_default();
                                move |_| fix(format!("the code failed: {error}"))
                            },
                            "Fix the code"
                        }
                    }
                },
                CellPhase::Stale => rsx! {
                    div { class: "conn-actions",
                        span { class: "conn-meta", "The description changed since this code was written." }
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Outline,
                            disabled: busy(),
                            onclick: move |_| rewrite("the description (or config) changed since the code was written: rewrite it to match".to_string()),
                            "Rewrite to match"
                        }
                    }
                },
                _ => rsx! {},
            }
            if reporting() {
                div { class: "conn-subform nb-report",
                    div { class: "orgs-field conn-wide",
                        Label { html_for: "nb-report-{cell.id}", "What should it do differently?" }
                        Textarea {
                            id: "nb-report-{cell.id}",
                            rows: 2,
                            placeholder: "It counts the shipping twice; the total for FR should be 12.",
                            value: report(),
                            oninput: move |e: FormEvent| report.set(e.value()),
                        }
                    }
                    div { class: "conn-actions",
                        Button {
                            size: ButtonSize::Sm,
                            disabled: busy() || report().trim().is_empty(),
                            onclick: move |_| submit(report()),
                            if implementing { "Add to the writing in progress" } else { "Rewrite the code" }
                        }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| reporting.set(false), "Cancel" }
                    }
                    p { class: "conn-meta", "The current code keeps running until the rewrite is built." }
                }
            }
            if open() {
                match phase {
                    CellPhase::Writing => rsx! {
                        WritingView { slug: slug.clone(), project: project.clone(), graph: graph.clone(), cell: cell.clone(), mine: mine.clone(), run: run.clone() }
                    },
                    _ if has_code => rsx! {
                        CodeView { slug: slug.clone(), project: project.clone(), graph: graph.clone(), cell: cell.clone() }
                    },
                    _ => rsx! {},
                }
            }
            if let Some(e) = failure() {
                p { class: "orgs-error", "{e}" }
            }
        }
    }
}

/// lode writing a cell's code: the steps of the run that mention it, and the
/// unpublished code, refreshed as the run goes.
#[component]
fn WritingView(
    slug: String,
    project: String,
    graph: String,
    cell: Cell,
    mine: Vec<LodeEntry>,
    run: Vec<LodeEntry>,
) -> Element {
    let mut whole = use_signal(|| false);
    let steps = run.len();
    let code = use_resource(use_reactive!(|(steps,)| {
        let (slug, project, graph, id) = (
            slug.clone(),
            project.clone(),
            graph.clone(),
            cell.id.clone(),
        );
        async move {
            let _ = steps;
            cell_code(slug, project, graph, id)
                .await
                .map_err(|e| error_message(&e))
        }
    }));
    let shown: Vec<LodeEntry> = if whole() { run.clone() } else { mine.clone() };
    rsx! {
        div { class: "nb-writing",
            match code() {
                Some(Ok(c)) => match c.draft {
                    Some(draft) => rsx! {
                        p { class: "conn-meta", "The code as it is being written (not published yet):" }
                        pre { class: "nb-code nb-diff", {diff_lines(&draft)} }
                    },
                    None => rsx! { p { class: "conn-meta", "No unpublished code mentions this cell yet." } },
                },
                Some(Err(e)) => rsx! { p { class: "orgs-error", "{e}" } },
                None => rsx! { p { class: "conn-meta", "Reading the changes…" } },
            }
            div { class: "conn-actions",
                span { class: "conn-meta",
                    if whole() { "Every step of the run" } else { "The steps of the run that mention {cell.name}" }
                }
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Ghost,
                    onclick: move |_| whole.set(!whole()),
                    if whole() { "Only this cell" } else { "The whole run ({run.len()})" }
                }
            }
            if shown.is_empty() {
                p { class: "conn-meta", "Nothing yet: the project is being read." }
            }
            div { class: "nb-log",
                for e in shown.iter().rev().take(40) {
                    div { key: "{e.index}", class: "nb-log-entry nb-log-{e.kind}",
                        span { class: "nb-log-kind", "{e.kind}" }
                        div {
                            span { class: "nb-log-text", "{e.text}" }
                            if !e.detail.is_empty() && e.detail != e.text {
                                details { class: "nb-log-detail",
                                    summary { "details" }
                                    pre { class: "nb-code", "{e.detail}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A diff, a line per element so additions and removals can be coloured.
fn diff_lines(diff: &str) -> Element {
    rsx! {
        for (i, line) in diff.lines().enumerate() {
            span {
                key: "{i}",
                class: if line.starts_with('+') && !line.starts_with("+++") {
                    "nb-add"
                } else if line.starts_with('-') && !line.starts_with("---") {
                    "nb-del"
                } else if line.starts_with("@@") || line.starts_with("diff ") {
                    "nb-hunk"
                } else {
                    ""
                },
                "{line}\n"
            }
        }
    }
}

/// The code lun runs for a cell: its function in the published commit.
#[component]
fn CodeView(slug: String, project: String, graph: String, cell: Cell) -> Element {
    let id = cell.id.clone();
    let code = use_resource(move || {
        let (slug, project, graph, id) = (slug.clone(), project.clone(), graph.clone(), id.clone());
        async move {
            cell_code(slug, project, graph, id)
                .await
                .map_err(|e| error_message(&e))
        }
    });
    rsx! {
        div { class: "nb-writing",
            match code() {
                None => rsx! { p { class: "conn-meta", "Reading the code…" } },
                Some(Err(e)) => rsx! { p { class: "orgs-error", "{e}" } },
                Some(Ok(c)) => rsx! {
                    if let Some(path) = c.path.clone() {
                        p { class: "conn-meta", code { "{path}" } }
                    }
                    match c.published {
                        Some(source) => rsx! { pre { class: "nb-code", "{source}" } },
                        None => rsx! { p { class: "conn-meta", "The published commit has no code for it." } },
                    }
                },
            }
        }
    }
}

#[component]
fn ImplView(cell: Cell) -> Element {
    let Some(imp) = cell.implementation.clone() else {
        return rsx! {};
    };
    let missing = cell.cell_type.has_function() && imp.function.is_none();
    rsx! {
        div { class: "nb-impl",
            if let (Some(f), Some(sig)) = (imp.function.clone(), imp.signature.clone()) {
                div {
                    code { "{f} : {sig}" }
                    if let Some(m) = imp.module.clone() { span { class: "conn-meta", " in {m}" } }
                }
            }
            if !imp.effects.is_empty() {
                div { class: "nb-effects",
                    for e in imp.effects.iter() {
                        span { class: "nb-effect", "{e}" }
                    }
                }
            }
            if let Some(input) = imp.input.clone() {
                div { class: "conn-meta",
                    "input " code { "{input}" }
                    if let Some(t) = imp.input_type.clone() { " : " code { "{t}" } }
                }
            }
            if missing {
                p { class: "orgs-error", "No code was written for {cell.name}: implement again." }
            }
        }
    }
}

#[component]
fn Relations(
    node: GraphNode,
    nodes: Vec<GraphNode>,
    sources: Vec<u64>,
    sinks: Vec<u64>,
) -> Element {
    let (direct, closure) = dependencies(&nodes, node.id);
    let indirect: Vec<u64> = closure
        .iter()
        .copied()
        .filter(|c| !direct.contains(c))
        .collect();
    let dependents: Vec<u64> = nodes
        .iter()
        .filter(|n| n.args.contains(&node.id))
        .map(|n| n.id)
        .collect();
    rsx! {
        div { class: "nb-relations",
            span { class: "nb-node", "{node_label(&node)}" }
            if sources.contains(&node.id) { span { class: "nb-effect", "graph source" } }
            if sinks.contains(&node.id) { span { class: "nb-effect", "graph sink" } }
            if !direct.is_empty() { span { class: "conn-meta", "reads {labels(&nodes, &direct)}" } }
            if !indirect.is_empty() { span { class: "conn-meta", "· also depends on {labels(&nodes, &indirect)}" } }
            if !dependents.is_empty() { span { class: "conn-meta", "· read by {labels(&nodes, &dependents)}" } }
        }
    }
}

/// A node's last outcome: its value through the cell's renderer, its error,
/// or what it was skipped for.
#[component]
fn Outcome(cell: Cell, node: GraphNode) -> Element {
    let Some(outcome) = node.outcome.clone() else {
        return rsx! { p { class: "conn-meta", "no value yet" } };
    };
    if let Some(e) = outcome.error {
        return rsx! { pre { class: "nb-error", "{e}" } };
    }
    if let Some(s) = outcome.skipped {
        return rsx! { p { class: "conn-meta", "skipped: #{s} has no value" } };
    }
    let Some(value) = outcome.output else {
        return rsx! {};
    };
    if cell.cell_type == CellType::UiInput {
        return rsx! {};
    }
    rsx! {
        div { class: "nb-output",
            if cell.cell_type == CellType::UiSink {
                Output { format: cell.config.format.clone().unwrap_or_else(|| "json".to_string()), value }
            } else {
                JsonView { value }
            }
        }
    }
}

// ── Inputs, secrets and endpoints ───────────────────────────────────────

/// The widget of a `ui` input (§3.3), from its JSON type.
#[component]
fn InputWidget(
    slug: String,
    project: String,
    graph: String,
    cell: Cell,
    ready: bool,
    on_fed: EventHandler<FeedResult>,
) -> Element {
    let input_type = cell
        .implementation
        .as_ref()
        .and_then(|i| i.input_type.clone());
    // Draft choices must not turn a running numeric input into a string feed.
    // Keep using the adopted Lean input type until the replacement build lands.
    let active_choices = if input_type.as_deref().is_some_and(|ty| ty.trim() != "String") {
        Vec::new()
    } else { cell.config.choices.clone() };
    let widget = widget_for(input_type.as_deref().or(cell.config.output_type.as_deref()), &active_choices);
    let initial = match (&widget, &cell.last_input) {
        (_, None) => String::new(),
        (Widget::Text | Widget::Select(_), Some(Value::String(s))) => s.clone(),
        (Widget::Json, Some(v)) => pretty(v),
        (_, Some(v)) => v.to_string(),
    };
    let mut text = use_signal(|| initial);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let id = format!("nb-input-{}", cell.id);

    let parse = {
        let widget = widget.clone();
        move |raw: String| -> Result<Value, String> {
            match &widget {
                Widget::Text | Widget::Select(_) => Ok(Value::String(raw)),
                Widget::Toggle => Ok(Value::Bool(raw == "true")),
                Widget::Number {
                    integer: true,
                    natural,
                } => {
                    let n: i64 = raw
                        .trim()
                        .parse()
                        .map_err(|_| "a whole number".to_string())?;
                    if *natural && n < 0 {
                        return Err("a number ≥ 0".to_string());
                    }
                    Ok(Value::from(n))
                }
                Widget::Number { .. } => {
                    let f: f64 = raw.trim().parse().map_err(|_| "a number".to_string())?;
                    serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .ok_or_else(|| "a finite number".to_string())
                }
                Widget::Json => serde_json::from_str(&raw).map_err(|e| format!("JSON: {e}")),
            }
        }
    };
    let send = {
        let (slug, project, graph, cell_id) = (
            slug.clone(),
            project.clone(),
            graph.clone(),
            cell.id.clone(),
        );
        move |raw: String| {
            let (slug, project, graph, cell_id) = (
                slug.clone(),
                project.clone(),
                graph.clone(),
                cell_id.clone(),
            );
            let parsed = parse(raw);
            async move {
                let value = match parsed {
                    Ok(v) => v,
                    Err(e) => {
                        error.set(Some(e));
                        return;
                    }
                };
                busy.set(true);
                match feed_cell(slug, project, graph, cell_id, value).await {
                    Ok(r) => {
                        error.set(None);
                        on_fed.call(r);
                    }
                    Err(e) => error.set(Some(error_message(&e))),
                }
                busy.set(false);
            }
        }
    };
    let (send_a, send_b) = (send.clone(), send);
    let disabled = !ready || busy();

    rsx! {
        div { class: "nb-widget",
            if cell.stale {
                p { class: "conn-meta", "Values feed the current built input type. Regenerate the notebook to adopt a changed source type." }
            }
            match widget.clone() {
                Widget::Toggle => {
                    let on = text() == "true";
                    rsx! {
                        Button {
                            variant: if on { ButtonVariant::Primary } else { ButtonVariant::Outline },
                            disabled,
                            onclick: move |_| {
                                let next = if on { "false" } else { "true" }.to_string();
                                text.set(next.clone());
                                send_a(next)
                            },
                            if on { "On" } else { "Off" }
                        }
                    }
                }
                Widget::Select(choices) => rsx! {
                    div { class: "conn-select",
                        Select::<String> {
                            default_value: text(),
                            aria_label: "{cell.name}",
                            disabled,
                            on_value_change: move |v: Option<String>| {
                                if let Some(v) = v {
                                    text.set(v.clone());
                                    spawn(send_a(v));
                                }
                            },
                            for (i, c) in choices.iter().enumerate() {
                                SelectOption::<String> { key: "{c}", index: i, value: c.clone(), text_value: c.clone(), "{c}" }
                            }
                        }
                    }
                },
                Widget::Json => rsx! {
                    div { class: "orgs-field conn-wide",
                        Label { html_for: "{id}", "Value (JSON)" }
                        Textarea { id: "{id}", rows: 4, value: text(), oninput: move |e: FormEvent| text.set(e.value()) }
                    }
                    Button { disabled, onclick: move |_| send_b(text()), "Feed input" }
                },
                Widget::Number { integer, natural } => rsx! {
                    Label { html_for: "{id}", "{cell.name} value" }
                    div { class: "conn-picker",
                        Input {
                            id: "{id}",
                            r#type: "number",
                            step: if integer { "1" } else { "any" },
                            min: if natural { "0" } else { "" },
                            value: text(),
                            oninput: move |e: FormEvent| text.set(e.value()),
                        }
                        Button { disabled, onclick: move |_| send_b(text()), "Feed input" }
                    }
                },
                Widget::Text => rsx! {
                    Label { html_for: "{id}", "{cell.name} value" }
                    div { class: "conn-picker",
                        Input { id: "{id}", value: text(), oninput: move |e: FormEvent| text.set(e.value()) }
                        Button { disabled, onclick: move |_| send_b(text()), "Feed input" }
                    }
                },
            }
            if !ready {
                p { class: "conn-meta", "Implement the notebook to feed its inputs." }
            }
            if let Some(e) = error() {
                p { class: "orgs-error", "{e}" }
            }
        }
    }
}

/// A secret (§3.4): a write-only password field. There is no "show".
#[component]
fn SecretField(
    slug: String,
    project: String,
    graph: String,
    cell: Cell,
    on_set: EventHandler<()>,
) -> Element {
    let mut value = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let id = format!("nb-secret-{}", cell.id);
    let cell_id = cell.id.clone();
    let submit = move |evt: FormEvent| {
        let (slug, project, graph, cell_id) = (
            slug.clone(),
            project.clone(),
            graph.clone(),
            cell_id.clone(),
        );
        async move {
            evt.prevent_default();
            busy.set(true);
            match set_cell_secret(slug, project, graph, cell_id, value()).await {
                Ok(_) => {
                    value.set(String::new());
                    error.set(None);
                    on_set.call(());
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };
    rsx! {
        form { class: "conn-picker", onsubmit: submit,
            div { class: "orgs-field",
                Label { html_for: "{id}", if cell.secret_set { "Replace the value" } else { "Value" } }
                Input { id: "{id}", r#type: "password", autocomplete: "off", value: value(), oninput: move |e: FormEvent| value.set(e.value()) }
            }
            Button { r#type: "submit", disabled: busy() || value().is_empty(), "Set secret" }
        }
        p { class: "conn-meta",
            if cell.secret_set { "Set. Stored in the vault; nobody can read it back, not even you." } else { "Not set yet." }
        }
        if let Some(e) = error() {
            p { class: "orgs-error", "{e}" }
        }
    }
}

/// An endpoint (§3.5): its URL is shown once, when made; rotating makes a
/// new one and revokes the old.
#[component]
fn EndpointBox(slug: String, project: String, graph: String, cell: Cell) -> Element {
    let mut url = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let cell_id = cell.id.clone();
    let rotate = move |_| {
        let (slug, project, graph, cell_id) = (
            slug.clone(),
            project.clone(),
            graph.clone(),
            cell_id.clone(),
        );
        async move {
            match rotate_endpoint(slug, project, graph, cell_id).await {
                Ok(saved) => {
                    url.set(saved.endpoint_url);
                    error.set(None);
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        }
    };
    rsx! {
        if let Some(u) = url() {
            EndpointUrl { url: u }
        } else {
            p { class: "conn-meta",
                if cell.has_endpoint { "POST JSON to this cell's URL (shown once, when it was made)." } else { "No URL yet." }
            }
        }
        div { class: "conn-actions",
            Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, onclick: rotate,
                if cell.has_endpoint { "New URL (revokes the current one)" } else { "Make a URL" }
            }
        }
        if let Some(e) = error() {
            p { class: "orgs-error", "{e}" }
        }
    }
}

#[component]
fn EndpointUrl(url: String) -> Element {
    rsx! {
        div { class: "nb-endpoint",
            p { class: "conn-result ok", "Copy this URL now: it is not shown again. Whoever holds it can feed the input and read what changed." }
            code { class: "nb-url", "{url}" }
            p { class: "conn-meta", "curl -X POST -H 'content-type: application/json' -d '…' {url}" }
        }
    }
}

// ── Adding and editing cells ────────────────────────────────────────────

#[component]
fn CellEditor(
    slug: String,
    project: String,
    graph: String,
    existing: Option<Cell>,
    cells: Vec<Cell>,
    nodes: Vec<GraphNode>,
    channels: Vec<Channel>,
    storage: Vec<Connection>,
    connections: Vec<Connection>,
    on_saved: EventHandler<()>,
    on_cancel: Option<EventHandler<()>>,
) -> Element {
    let start = existing.clone();
    let mut cell_type = use_signal(|| {
        start
            .as_ref()
            .map(|c| c.cell_type)
            .unwrap_or(CellType::Node)
    });
    let mut name = use_signal(|| start.as_ref().map(|c| c.name.clone()).unwrap_or_default());
    let mut description = use_signal(|| {
        start
            .as_ref()
            .map(|c| c.description.clone())
            .unwrap_or_default()
    });
    let initial = start.as_ref().map(|c| c.config.clone()).unwrap_or_default();
    let field = |v: &Option<String>| v.clone().unwrap_or_default();
    let mut url = use_signal(|| field(&initial.url));
    let mut schedule = use_signal(|| {
        initial
            .schedule
            .clone()
            .unwrap_or_else(|| "0 * * * *".to_string())
    });
    let mut input = use_signal(|| field(&initial.input));
    let mut dependency_names = use_signal(|| start.as_ref().map(|cell| cell_dependencies(cell, &cells, &nodes).join(", ")).unwrap_or_default());
    let mut output_type = use_signal(|| field(&initial.output_type));
    let mut connectors = use_signal(|| initial.connectors.clone());
    let mut choices = use_signal(|| initial.choices.join(", "));
    let mut secret = use_signal(|| field(&initial.name));
    let mut channel = use_signal(|| {
        initial
            .channel_id
            .clone()
            .or_else(|| channels.first().map(|c| c.id.clone()))
            .unwrap_or_default()
    });
    let mut recipient = use_signal(|| field(&initial.recipient));
    let mut table = use_signal(|| field(&initial.table));
    let mut connection = use_signal(|| {
        initial
            .connection_id
            .clone()
            .or_else(|| storage.first().map(|c| c.id.clone()))
            .unwrap_or_default()
    });
    let mut path = use_signal(|| field(&initial.path));
    let mut format = use_signal(|| {
        initial
            .format
            .clone()
            .unwrap_or_else(|| "table".to_string())
    });
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut shown_url = use_signal(|| None::<String>);
    let policy_slug = slug.clone();
    let permissions = use_server_future(move || api::get_org_settings(policy_slug.clone()))?;
    let editing = existing.is_some();
    let prefix = existing
        .as_ref()
        .map(|c| c.id.clone())
        .unwrap_or_else(|| "new".to_string());

    let config = move || {
        let opt = |s: String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        CellConfig {
            connectors: connectors(),
            dependencies: Some(if cell_type().is_source() { Vec::new() } else { dependency_names().split([',', ' ', '\n']).filter(|s| !s.is_empty()).map(|s| s.trim_start_matches('@').to_string()).collect() }),
            output_type: opt(output_type()),
            url: opt(url()),
            schedule: opt(schedule()),
            input: opt(input()),
            choices: choices()
                .split(',')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect(),
            name: opt(secret()),
            channel_id: opt(channel()),
            recipient: opt(recipient()),
            table: opt(table()),
            connection_id: opt(connection()),
            path: opt(path()),
            format: opt(format()),
        }
    };
    let existing_id = existing.as_ref().map(|c| c.id.clone());
    // Where to set up what a cell needs, when it is missing.
    let interfaces_href = format!("/orgs/{slug}/projects/{project}/settings/interfaces");
    let connections_href = format!("/orgs/{slug}/settings/connections");
    let submit = move |evt: FormEvent| {
        let submitted_config = config();
        let (slug, project, graph, existing_id) = (
            slug.clone(),
            project.clone(),
            graph.clone(),
            existing_id.clone(),
        );
        async move {
            evt.prevent_default();
            let (t, n, d, c) = (cell_type(), name(), description(), submitted_config);
            if let Err(e) = validate_cell(t, &n, &d, &c) {
                error.set(Some(e));
                return;
            }
            busy.set(true);
            let result = match existing_id {
                Some(id) => update_cell(slug, project, graph, id, n, d, c)
                    .await
                    .map(|_| None),
                None => add_cell(slug, project, graph, t, n, d, c)
                    .await
                    .map(|s| s.endpoint_url),
            };
            match result {
                Ok(endpoint) => {
                    error.set(None);
                    if !editing {
                        name.set(String::new());
                        description.set(String::new());
                        input.set(String::new());
                        dependency_names.set(String::new());
                        output_type.set(String::new());
                        connectors.set(Vec::new());
                    }
                    shown_url.set(endpoint);
                    on_saved.call(());
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
            busy.set(false);
        }
    };

    let t = cell_type();
    rsx! {
        form { class: "conn-section nb-editor", onsubmit: submit,
            h4 { if editing { "Edit the cell" } else { "Add a cell" } }
            if !editing {
                div { class: "conn-select conn-select-wide",
                    Select::<CellType> {
                        default_value: cell_type(),
                        aria_label: "Kind of cell",
                        on_value_change: move |v: Option<CellType>| {
                            if let Some(v) = v {
                                cell_type.set(v);
                            }
                        },
                        for (i, k) in CellType::ALL.into_iter().enumerate() {
                            SelectOption::<CellType> { key: "{k.id()}", index: i, value: k, text_value: k.label(), "{k.label()}" }
                        }
                    }
                }
            }
            div { class: "conn-grid",
                div { class: "orgs-field",
                    Label { html_for: "{prefix}-name", "Name" }
                    Input { id: "{prefix}-name", placeholder: "total", value: name(), oninput: move |e: FormEvent| name.set(e.value()) }
                }
                if t.has_input() {
                    div { class: "orgs-field",
                        Label { html_for: "{prefix}-input", "Input" }
                        Input { id: "{prefix}-input", placeholder: "defaults to the name", value: input(), oninput: move |e: FormEvent| input.set(e.value()) }
                    }
                }
            }
            div { class: "orgs-field conn-wide",
                Label { html_for: "{prefix}-desc", "Description" }
                Textarea {
                    id: "{prefix}-desc",
                    rows: 3,
                    placeholder: placeholder(t),
                    value: description(),
                    oninput: move |e: FormEvent| description.set(e.value()),
                }
            }
            if !t.is_source() {
                div { class: "orgs-field conn-wide",
                    Label { html_for: "{prefix}-dependencies", "Input cells (in argument order)" }
                    Input { id: "{prefix}-dependencies", placeholder: "price, tax", value: dependency_names(), oninput: move |e: FormEvent| dependency_names.set(e.value()) }
                    p { class: "conn-meta", "Refer to cells by name, or write @cell_name in the description. Leave empty for no dependencies." }
                    div { class: "nb-reference-picker",
                        for other in cells.iter().filter(|c| Some(&c.id) != existing.as_ref().map(|e| &e.id) && c.cell_type != CellType::Secret) {
                            Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Ghost,
                                onclick: {
                                    let reference = other.name.clone();
                                    move |_| {
                                        let mut names: Vec<String> = dependency_names().split([',', ' ', '\n']).filter(|s| !s.is_empty()).map(|s| s.trim_start_matches('@').to_string()).collect();
                                        if !names.contains(&reference) { names.push(reference.clone()); dependency_names.set(names.join(", ")); }
                                    }
                                }, "@{other.name}" }
                        }
                    }
                }
            }
            if t != CellType::Secret {
                div { class: "orgs-field conn-wide",
                    Label { html_for: "{prefix}-output-type", "Output type (optional Lean 4 constraint)" }
                    Input { id: "{prefix}-output-type", placeholder: "Nat, String, List Invoice…", value: output_type(), oninput: move |e: FormEvent| output_type.set(e.value()) }
                    p { class: "conn-meta",
                        if t.has_function() { "Argument types come from the input cells. The generated function must return Eff effs T with this output type." }
                        else { "Constrains the source's input value and widget. The runtime must validate fed values against this Lean type." }
                    }
                }
            }
            match t {
                CellType::Scheduled | CellType::Watch => rsx! {
                    div { class: "conn-grid",
                        div { class: "orgs-field",
                            Label { html_for: "{prefix}-url", "Page URL" }
                            Input { id: "{prefix}-url", placeholder: "https://example.com/prices", value: url(), oninput: move |e: FormEvent| url.set(e.value()) }
                        }
                        div { class: "orgs-field",
                            Label { html_for: "{prefix}-cron", "Schedule (cron, UTC)" }
                            Input { id: "{prefix}-cron", placeholder: "0 9 * * 1-5", value: schedule(), oninput: move |e: FormEvent| schedule.set(e.value()) }
                        }
                    }
                    p { class: "conn-meta", "Five fields: minute hour day month weekday. Public pages only; private addresses are refused." }
                },
                CellType::UiInput => rsx! {
                    div { class: "orgs-field conn-wide",
                        Label { html_for: "{prefix}-choices", "Choices (optional, comma-separated)" }
                        Input { id: "{prefix}-choices", placeholder: "FR, DE, IT", value: choices(), oninput: move |e: FormEvent| choices.set(e.value()) }
                    }
                },
                CellType::Secret => rsx! {
                    div { class: "orgs-field",
                        Label { html_for: "{prefix}-secret", "Secret name" }
                        Input { id: "{prefix}-secret", placeholder: "pricing_api_key", value: secret(), oninput: move |e: FormEvent| secret.set(e.value()) }
                    }
                    p { class: "conn-meta", "Set the value once the cell exists; it goes to the vault and is never shown again." }
                },
                CellType::ChannelSource | CellType::ChannelSink => rsx! {
                    if channels.is_empty() {
                        p { class: "orgs-error", "This project has no messaging interface yet: ", Link { to: "{interfaces_href}", "add one in its settings" }, "." }
                    } else {
                        div { class: "orgs-field conn-select conn-select-wide",
                            Label { html_for: "{prefix}-channel", "Interface" }
                            Select::<String> {
                                default_value: channel(),
                                aria_label: "Interface",
                                on_value_change: move |v: Option<String>| {
                                    if let Some(v) = v {
                                        channel.set(v);
                                    }
                                },
                                for (i, c) in channels.iter().enumerate() {
                                    SelectOption::<String> { key: "{c.id}", index: i, value: c.id.clone(), text_value: c.label.clone(), "{c.provider.name()} · {c.label}" }
                                }
                            }
                        }
                        if t == CellType::ChannelSink && channels.iter().find(|c| c.id == channel()).is_some_and(|c| c.provider != Provider::Slack) {
                            div { class: "orgs-field",
                                Label { html_for: "{prefix}-to", "Send to (phone number)" }
                                Input { id: "{prefix}-to", placeholder: "+33 6 12 34 56 78", value: recipient(), oninput: move |e: FormEvent| recipient.set(e.value()) }
                            }
                        }
                    }
                },
                CellType::DbSink => rsx! {
                    div { class: "orgs-field",
                        Label { html_for: "{prefix}-table", "Table" }
                        Input { id: "{prefix}-table", placeholder: "totals", value: table(), oninput: move |e: FormEvent| table.set(e.value()) }
                    }
                    p { class: "conn-meta", "In your own database schema: only your notebooks, as you, can write there." }
                },
                CellType::HttpSink => rsx! {
                    div { class: "orgs-field conn-wide",
                        Label { html_for: "{prefix}-url", "POST to" }
                        Input { id: "{prefix}-url", placeholder: "https://hooks.example.com/incidents", value: url(), oninput: move |e: FormEvent| url.set(e.value()) }
                    }
                },
                CellType::StorageSink => rsx! {
                    if storage.is_empty() {
                        p { class: "orgs-error", "No storage account yet: ", Link { to: "{connections_href}", "connect one in the organisation's settings" }, "." }
                    } else {
                        div { class: "conn-grid",
                            div { class: "orgs-field conn-select",
                                Label { html_for: "{prefix}-conn", "Storage" }
                                Select::<String> {
                                    default_value: connection(),
                                    aria_label: "Storage connection",
                                    on_value_change: move |v: Option<String>| {
                                        if let Some(v) = v {
                                            connection.set(v);
                                        }
                                    },
                                    for (i, c) in storage.iter().enumerate() {
                                        SelectOption::<String> { key: "{c.id}", index: i, value: c.id.clone(), text_value: c.label.clone(), "{c.provider.name()} · {c.label}" }
                                    }
                                }
                            }
                            div { class: "orgs-field",
                                Label { html_for: "{prefix}-path", "Path" }
                                Input { id: "{prefix}-path", placeholder: "reports/latest.json", value: path(), oninput: move |e: FormEvent| path.set(e.value()) }
                            }
                        }
                    }
                },
                CellType::UiSink => rsx! {
                    div { class: "orgs-field conn-select",
                        Label { html_for: "{prefix}-format", "Shown as" }
                        Select::<String> {
                            default_value: format(),
                            aria_label: "Format",
                            on_value_change: move |v: Option<String>| {
                                if let Some(v) = v {
                                    format.set(v);
                                }
                            },
                            for (i, f) in UI_FORMATS.iter().enumerate() {
                                SelectOption::<String> { key: "{f}", index: i, value: f.to_string(), text_value: f.to_string(), "{f}" }
                            }
                        }
                    }
                },
                _ => rsx! {},
            }
            if t != CellType::Secret {
                CellConnections { connections: connections.clone(), value: connectors(), id: prefix.clone(), disabled: busy(),
                    policy: permissions().and_then(Result::ok).map(|s| s.effect_policy),
                    storage_scope: if t == CellType::StorageSink { Some((connection(), path())) } else { None },
                    on_change: move |value: Vec<CellConnector>| connectors.set(value) }
            }
            div { class: "conn-actions",
                Button { r#type: "submit", disabled: busy(), if editing { "Save" } else { "Add cell" } }
                if let Some(cancel) = on_cancel {
                    Button { r#type: "button", variant: ButtonVariant::Ghost, onclick: move |_| cancel.call(()), "Cancel" }
                }
            }
            if let Some(u) = shown_url() {
                EndpointUrl { url: u }
            }
            if let Some(e) = error() {
                p { class: "orgs-error", "{e}" }
            }
        }
    }
}

#[component]
fn CellConnections(connections: Vec<Connection>, value: Vec<CellConnector>, id: String, disabled: bool, policy: Option<api::EffectPolicy>, storage_scope: Option<(String, String)>, on_change: EventHandler<Vec<CellConnector>>) -> Element {
    rsx! {
        details { class: "cell-connections",
            summary { "Connections for this cell ({value.len()})" }
            p { class: "conn-meta", "Select the accounts this cell may use. Permissions inherit each connection's ceiling; Advanced can narrow operations and resources. Organization policy and runtime warrants remain upper bounds." }
            if connections.is_empty() { p { class: "conn-meta", "No connections available. Add one in the organization's connection settings." } }
            for connection in &connections {
                {let parent = connection.permissions.clone().unwrap_or_else(|| ConnectorPermissions::preset(connection.provider, PermissionPreset::ReadOnly));
                let ceiling = policy.as_ref().and_then(|p| p.connector_ceilings.get(connection.provider.id())).map_or(parent.clone(), |org| parent.intersect(org));
                let allowed = policy.as_ref().is_some_and(|p| p.effects.iter().any(|e| e == "Connector") && p.allows_provider(connection.provider));
                rsx! {
                div { class: "cell-connection", key: "{connection.id}",
                    Button { r#type: "button", size: ButtonSize::Sm, variant: if value.iter().any(|grant| grant.connection == connection.id) { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                        disabled: disabled || (!allowed && !value.iter().any(|grant| grant.connection == connection.id)), aria_pressed: value.iter().any(|grant| grant.connection == connection.id).to_string(),
                        onclick: { let mut next = value.clone(); let connection = connection.id.clone(); move |_| {
                            if next.iter().any(|grant| grant.connection == connection) { next.retain(|grant| grant.connection != connection); }
                            else { next.push(CellConnector { connection: connection.clone(), permissions: None }); }
                            on_change.call(next.clone());
                        } }, "{connection_name(connection)}" }
                    if let Some(grant) = value.iter().find(|grant| grant.connection == connection.id) {
                        PermissionEditor { id: "{id}-{connection.id}", provider: connection.provider, value: grant.permissions.clone(), disabled: disabled || !allowed,
                            ceiling: Some(ceiling.clone()),
                            on_change: { let mut next = value.clone(); let connection = connection.id.clone(); move |permissions| {
                                if let Some(grant) = next.iter_mut().find(|grant| grant.connection == connection) { grant.permissions = permissions; }
                                on_change.call(next.clone());
                            } } }
                        if let Some((storage_id, path)) = storage_scope.as_ref().filter(|(storage_id, path)| storage_id == &connection.id && !path.trim().is_empty() && matches!(connection.provider, Provider::S3 | Provider::Azure)) {
                            Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Outline, disabled,
                                onclick: { let mut next = value.clone(); let connection = storage_id.clone(); let root: Vec<String> = path.trim().split('/').map(str::to_string).collect(); let ceiling = ceiling.clone(); move |_| {
                                    if let Some(grant) = next.iter_mut().find(|g| g.connection == connection) {
                                        let mut requested = ceiling.clone();
                                        for scope in &mut requested.scopes { scope.root = root.clone(); scope.descendants = false; }
                                        grant.permissions = Some(ceiling.intersect(&requested));
                                    }
                                    on_change.call(next.clone());
                                } }, "Use configured object: {path}" }
                        }
                    }
                    if !allowed { p { class: "conn-meta", "Disabled by organization notebook permissions." } }
                }
                }}
            }
            for grant in value.iter().filter(|grant| !connections.iter().any(|connection| connection.id == grant.connection)) {
                p { class: "orgs-error", "Unavailable connection: {grant.connection}" }
                Button { r#type: "button", size: ButtonSize::Sm, variant: ButtonVariant::Ghost, disabled,
                    onclick: { let mut next = value.clone(); let connection = grant.connection.clone(); move |_| { next.retain(|grant| grant.connection != connection); on_change.call(next.clone()); } },
                    "Remove unavailable connection" }
            }
        }
    }
}

fn placeholder(t: CellType) -> &'static str {
    match t {
        CellType::Node => "Double the amount.",
        CellType::Scheduled => {
            "Read the first table of the page: the price of each product, in euros."
        }
        CellType::Watch => "The price of the Pro plan on the page.",
        CellType::UiInput => "The country to ship to.",
        CellType::Secret => "The API key of the pricing service.",
        CellType::Endpoint => "GitHub's push webhooks: the repository and the pusher.",
        CellType::ChannelSource => "Each message is an order: the first number is the amount.",
        CellType::DbSink => "Write the total, with the date, to the totals table.",
        CellType::HttpSink => "POST the alert when the price drops under 10.",
        CellType::StorageSink => "Archive the report as JSON.",
        CellType::UiSink => "Show the invoice lines as a table.",
        CellType::ChannelSink => "Reply with the confirmation: the order number and the total.",
    }
}

// ── Activity ────────────────────────────────────────────────────────────

#[component]
fn ActivityCard(detail: GraphDetail) -> Element {
    if detail.activity.is_empty() {
        return rsx! {};
    }
    rsx! {
        Card {
            CardHeader {
                CardTitle { "Activity" }
                CardDescription { "Updates, checks and webhook deliveries, newest first." }
            }
            CardContent {
                div { class: "nb-activity",
                    for (i, a) in detail.activity.iter().enumerate() {
                        div { key: "{i}", class: if a.ok { "nb-activity-row" } else { "nb-activity-row err" },
                            span { class: "conn-meta", "{a.at}" }
                            span { class: "nb-effect", "{a.kind}" }
                            if let Some(c) = a.cell.clone() { code { "{c}" } }
                            span { "{a.text}" }
                        }
                    }
                }
            }
        }
    }
}
