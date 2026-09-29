//! The notebook's `ui` sink renderers (docs/computations.md §4.4): a table,
//! a chart, and Markdown — each over a typed model parsed from the node's
//! JSON, rendered as RSX elements and text nodes.
//!
//! **No markup from a graph ever reaches the page.** A graph's code is user
//! code; its output is data. Nothing here uses `dangerous_inner_html`, and
//! the Markdown subset has no HTML at all: `<b>` in a node's output is shown
//! as the four characters `<b>`. Links are kept only for `https:`, `http:`
//! and `mailto:` targets (and open in a new tab without an opener). An
//! unknown format renders as the raw JSON.

use dioxus::prelude::*;
use serde_json::Value;

// ── Models ──────────────────────────────────────────────────────────────

/// A value as a table cell's text.
fn cell_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A table: columns and rows of text. From an array of objects (columns are
/// the keys, in first-seen order), an array of arrays (the first row is the
/// header when it is all strings), or an object (key, value).
pub fn table_model(v: &Value) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    match v {
        Value::Array(rows) if rows.iter().all(Value::is_object) && !rows.is_empty() => {
            let mut columns: Vec<String> = Vec::new();
            for r in rows {
                for k in r.as_object()?.keys() {
                    if !columns.contains(k) {
                        columns.push(k.clone());
                    }
                }
            }
            let body = rows
                .iter()
                .map(|r| {
                    columns
                        .iter()
                        .map(|c| r.get(c).map(cell_text).unwrap_or_default())
                        .collect()
                })
                .collect();
            Some((columns, body))
        }
        Value::Array(rows) if rows.iter().all(Value::is_array) && !rows.is_empty() => {
            let mut rows: Vec<Vec<String>> = rows
                .iter()
                .map(|r| {
                    r.as_array()
                        .map(|c| c.iter().map(cell_text).collect())
                        .unwrap_or_default()
                })
                .collect();
            let header_like = v[0]
                .as_array()
                .is_some_and(|h| h.iter().all(Value::is_string));
            let width = rows.iter().map(Vec::len).max().unwrap_or(0);
            let columns = if header_like && rows.len() > 1 {
                rows.remove(0)
            } else {
                (1..=width).map(|i| i.to_string()).collect()
            };
            Some((columns, rows))
        }
        Value::Array(items) if !items.is_empty() => Some((
            vec!["value".to_string()],
            items.iter().map(|i| vec![cell_text(i)]).collect(),
        )),
        Value::Object(map) if !map.is_empty() => Some((
            vec!["key".to_string(), "value".to_string()],
            map.iter()
                .map(|(k, v)| vec![k.clone(), cell_text(v)])
                .collect(),
        )),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChartKind {
    Line,
    Bar,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub kind: ChartKind,
    pub labels: Vec<String>,
    pub series: Vec<(String, Vec<f64>)>,
}

/// A chart, from `{"type", "labels", "series": [{"name", "values"}]}`, an
/// array of numbers, an array of `{x|label, y|value}`, or an object of
/// numbers (a bar per key).
pub fn chart_model(v: &Value) -> Option<Chart> {
    let nums =
        |a: &Value| -> Option<Vec<f64>> { a.as_array()?.iter().map(Value::as_f64).collect() };
    if let Some(series) = v.get("series").and_then(Value::as_array) {
        let kind = match v.get("type").and_then(Value::as_str) {
            Some("bar") => ChartKind::Bar,
            _ => ChartKind::Line,
        };
        let series: Vec<(String, Vec<f64>)> = series
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let values = nums(s.get("values")?)?;
                let name = s
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("series {}", i + 1));
                Some((name, values))
            })
            .collect();
        let n = series.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
        let labels = v
            .get("labels")
            .and_then(Value::as_array)
            .map(|l| l.iter().map(cell_text).collect())
            .unwrap_or_else(|| (1..=n).map(|i| i.to_string()).collect());
        return (n > 0).then_some(Chart {
            kind,
            labels,
            series,
        });
    }
    if let Some(values) = nums(v).filter(|v| !v.is_empty()) {
        return Some(Chart {
            kind: ChartKind::Line,
            labels: (1..=values.len()).map(|i| i.to_string()).collect(),
            series: vec![(String::new(), values)],
        });
    }
    if let Some(points) = v.as_array().filter(|a| !a.is_empty()) {
        let mut labels = Vec::new();
        let mut values = Vec::new();
        for p in points {
            let x = p.get("x").or_else(|| p.get("label"))?;
            let y = p.get("y").or_else(|| p.get("value"))?.as_f64()?;
            labels.push(cell_text(x));
            values.push(y);
        }
        return Some(Chart {
            kind: ChartKind::Line,
            labels,
            series: vec![(String::new(), values)],
        });
    }
    if let Some(map) = v.as_object().filter(|m| !m.is_empty()) {
        let values: Option<Vec<f64>> = map.values().map(Value::as_f64).collect();
        return Some(Chart {
            kind: ChartKind::Bar,
            labels: map.keys().cloned().collect(),
            series: vec![(String::new(), values?)],
        });
    }
    None
}

/// Inline Markdown: text, `code`, **strong**, *emphasis*, [links](url).
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(String),
    Em(String),
    Link { text: String, href: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(u8, Vec<Inline>),
    Paragraph(Vec<Inline>),
    List {
        ordered: bool,
        items: Vec<Vec<Inline>>,
    },
    Code(String),
    Quote(Vec<Inline>),
    Rule,
}

/// The link targets kept: anything else becomes plain text.
pub fn safe_href(href: &str) -> Option<String> {
    let h = href.trim();
    let lower = h.to_ascii_lowercase();
    let ok = (lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:"))
        && !h
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '"' || c == '<' || c == '>');
    ok.then(|| h.to_string())
}

fn push_text(out: &mut Vec<Inline>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Inline::Text(t)) = out.last_mut() {
        t.push_str(s);
    } else {
        out.push(Inline::Text(s.to_string()));
    }
}

pub fn inlines(s: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let next = rest.find(['`', '*', '_', '[']);
        let Some(at) = next else {
            push_text(&mut out, rest);
            break;
        };
        push_text(&mut out, &rest[..at]);
        let tail = &rest[at..];
        let delimited = |open: &str, close: &str| -> Option<(String, usize)> {
            let inner = tail.strip_prefix(open)?;
            let end = inner.find(close)?;
            (end > 0).then(|| (inner[..end].to_string(), open.len() + end + close.len()))
        };
        let parsed = if tail.starts_with('`') {
            delimited("`", "`").map(|(t, n)| (Inline::Code(t), n))
        } else if tail.starts_with("**") {
            delimited("**", "**").map(|(t, n)| (Inline::Strong(t), n))
        } else if tail.starts_with("__") {
            delimited("__", "__").map(|(t, n)| (Inline::Strong(t), n))
        } else if tail.starts_with('*') {
            delimited("*", "*").map(|(t, n)| (Inline::Em(t), n))
        } else if tail.starts_with('_') {
            delimited("_", "_").map(|(t, n)| (Inline::Em(t), n))
        } else {
            // [text](href)
            tail.find("](").and_then(|mid| {
                let close = tail[mid + 2..].find(')')? + mid + 2;
                let text = tail[1..mid].to_string();
                let href = &tail[mid + 2..close];
                let inline = match safe_href(href) {
                    Some(href) if !text.is_empty() => Inline::Link { text, href },
                    _ => Inline::Text(tail[..=close].to_string()),
                };
                Some((inline, close + 1))
            })
        };
        match parsed {
            Some((Inline::Text(t), n)) => {
                push_text(&mut out, &t);
                rest = &tail[n..];
            }
            Some((inline, n)) => {
                out.push(inline);
                rest = &tail[n..];
            }
            None => {
                let c = tail.chars().next().map(char::len_utf8).unwrap_or(1);
                push_text(&mut out, &tail[..c]);
                rest = &tail[c..];
            }
        }
    }
    out
}

fn list_item(line: &str) -> Option<(bool, &str)> {
    let t = line.trim_start();
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(bullet) {
            return Some((false, rest));
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        if let Some(rest) = t[digits..].strip_prefix(". ") {
            return Some((true, rest));
        }
    }
    None
}

/// Block Markdown: `#` headings, paragraphs, `-`/`1.` lists, fenced code,
/// `>` quotes and `---` rules.
pub fn markdown(src: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            blocks.push(Block::Paragraph(inlines(&para.join(" "))));
            para.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        if t.starts_with("```") {
            flush(&mut para, &mut blocks);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            blocks.push(Block::Code(code.join("\n")));
            i += 1;
            continue;
        }
        if t.is_empty() {
            flush(&mut para, &mut blocks);
        } else if t.starts_with('#') && t.trim_start_matches('#').starts_with(' ') {
            flush(&mut para, &mut blocks);
            let level = t.chars().take_while(|c| *c == '#').count().min(6) as u8;
            blocks.push(Block::Heading(
                level,
                inlines(t.trim_start_matches('#').trim()),
            ));
        } else if t == "---" || t == "***" || t == "___" {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Rule);
        } else if let Some(q) = t.strip_prefix('>') {
            flush(&mut para, &mut blocks);
            let mut quote = vec![q.trim()];
            while i + 1 < lines.len() && lines[i + 1].trim().starts_with('>') {
                i += 1;
                quote.push(lines[i].trim().trim_start_matches('>').trim());
            }
            blocks.push(Block::Quote(inlines(&quote.join(" "))));
        } else if let Some((ordered, first)) = list_item(line) {
            flush(&mut para, &mut blocks);
            let mut items = vec![inlines(first)];
            while i + 1 < lines.len() {
                match list_item(lines[i + 1]) {
                    Some((o, item)) if o == ordered => {
                        items.push(inlines(item));
                        i += 1;
                    }
                    _ => break,
                }
            }
            blocks.push(Block::List { ordered, items });
        } else {
            para.push(t);
        }
        i += 1;
    }
    flush(&mut para, &mut blocks);
    blocks
}

// ── Rendering ───────────────────────────────────────────────────────────

pub fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

/// A node's output in the notebook: through the renderer `format` names,
/// or as raw JSON when the format is unknown or the value does not fit it.
#[component]
pub(crate) fn Output(format: String, value: Value) -> Element {
    match format.as_str() {
        "table" => match table_model(&value) {
            Some((columns, rows)) => rsx! { TableView { columns, rows } },
            None => rsx! { JsonView { value } },
        },
        "chart" => match chart_model(&value) {
            Some(chart) => rsx! { ChartView { chart } },
            None => rsx! { JsonView { value } },
        },
        "markdown" => match &value {
            Value::String(s) => rsx! { MarkdownView { blocks: markdown(s) } },
            _ => rsx! { JsonView { value } },
        },
        _ => rsx! { JsonView { value } },
    }
}

#[component]
pub(crate) fn JsonView(value: Value) -> Element {
    rsx! { pre { class: "nb-json", "{pretty(&value)}" } }
}

#[component]
fn TableView(columns: Vec<String>, rows: Vec<Vec<String>>) -> Element {
    rsx! {
        div { class: "nb-table-wrap",
            table { class: "orgs-table nb-table",
                thead {
                    tr {
                        for c in columns.iter() {
                            th { "{c}" }
                        }
                    }
                }
                tbody {
                    for row in rows.iter() {
                        tr {
                            for c in row.iter() {
                                td { "{c}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

const PALETTE: [&str; 5] = [
    "var(--app-accent)",
    "var(--app-success)",
    "var(--app-warning)",
    "var(--app-danger)",
    "var(--app-muted)",
];

#[component]
fn ChartView(chart: Chart) -> Element {
    let (w, h, pad) = (600.0f64, 240.0f64, 32.0f64);
    let all: Vec<f64> = chart
        .series
        .iter()
        .flat_map(|(_, v)| v.iter().copied())
        .filter(|v| v.is_finite())
        .collect();
    let max = all.iter().copied().fold(f64::MIN, f64::max).max(0.0);
    let min = all.iter().copied().fold(f64::MAX, f64::min).min(0.0);
    let span = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };
    let n = chart
        .labels
        .len()
        .max(chart.series.iter().map(|(_, v)| v.len()).max().unwrap_or(0))
        .max(1);
    let x = move |i: usize| {
        pad + (w - 2.0 * pad)
            * if n == 1 {
                0.5
            } else {
                i as f64 / (n - 1) as f64
            }
    };
    let y = move |v: f64| h - pad - (h - 2.0 * pad) * (v - min) / span;
    let zero = y(0.0);
    let slot = (w - 2.0 * pad) / n as f64;
    let bars = chart.series.len().max(1) as f64;
    let every = (n / 8).max(1);
    rsx! {
        div { class: "nb-chart",
            svg {
                view_box: "0 0 {w} {h}",
                role: "img",
                "aria-label": "chart",
                line { x1: "{pad}", y1: "{zero}", x2: "{w - pad}", y2: "{zero}", stroke: "var(--app-border-strong)", stroke_width: "1" }
                text { x: "4", y: "{pad}", font_size: "10", fill: "var(--app-muted)", "{max}" }
                text { x: "4", y: "{h - pad}", font_size: "10", fill: "var(--app-muted)", "{min}" }
                for (s, (_, values)) in chart.series.iter().enumerate() {
                    if chart.kind == ChartKind::Line {
                        polyline {
                            fill: "none",
                            stroke: PALETTE[s % PALETTE.len()],
                            stroke_width: "2",
                            points: values.iter().enumerate().filter(|(_, v)| v.is_finite()).map(|(i, v)| format!("{:.1},{:.1}", x(i), y(*v))).collect::<Vec<_>>().join(" "),
                        }
                    } else {
                        for (i, v) in values.iter().enumerate().filter(|(_, v)| v.is_finite()) {
                            rect {
                                x: "{pad + slot * i as f64 + slot * 0.1 + slot * 0.8 / bars * s as f64:.1}",
                                y: "{y(v.max(0.0)):.1}",
                                width: "{(slot * 0.8 / bars).max(1.0):.1}",
                                height: "{(y(v.min(0.0)) - y(v.max(0.0))).abs():.1}",
                                fill: PALETTE[s % PALETTE.len()],
                            }
                        }
                    }
                }
                for (i, label) in chart.labels.iter().enumerate().filter(|(i, _)| i % every == 0) {
                    text {
                        x: if chart.kind == ChartKind::Bar { format!("{:.1}", pad + slot * (i as f64 + 0.5)) } else { format!("{:.1}", x(i)) },
                        y: "{h - 10.0}",
                        font_size: "10",
                        text_anchor: "middle",
                        fill: "var(--app-muted)",
                        "{label}"
                    }
                }
            }
            if chart.series.len() > 1 || chart.series.iter().any(|(n, _)| !n.is_empty()) {
                div { class: "nb-legend",
                    for (s, (name, _)) in chart.series.iter().enumerate() {
                        span { class: "nb-legend-item",
                            span { class: "nb-swatch", style: "background: {PALETTE[s % PALETTE.len()]}" }
                            "{name}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn InlineView(parts: Vec<Inline>) -> Element {
    rsx! {
        for part in parts {
            match part {
                Inline::Text(t) => rsx! { "{t}" },
                Inline::Code(t) => rsx! { code { "{t}" } },
                Inline::Strong(t) => rsx! { strong { "{t}" } },
                Inline::Em(t) => rsx! { em { "{t}" } },
                Inline::Link { text, href } => rsx! {
                    a { href: "{href}", target: "_blank", rel: "noopener noreferrer nofollow", "{text}" }
                },
            }
        }
    }
}

#[component]
fn MarkdownView(blocks: Vec<Block>) -> Element {
    rsx! {
        div { class: "nb-markdown",
            for block in blocks {
                match block {
                    Block::Heading(1, parts) => rsx! { h3 { InlineView { parts } } },
                    Block::Heading(2, parts) => rsx! { h4 { InlineView { parts } } },
                    Block::Heading(_, parts) => rsx! { h5 { InlineView { parts } } },
                    Block::Paragraph(parts) => rsx! { p { InlineView { parts } } },
                    Block::Quote(parts) => rsx! { blockquote { InlineView { parts } } },
                    Block::Code(code) => rsx! { pre { code { "{code}" } } },
                    Block::Rule => rsx! { hr {} },
                    Block::List { ordered: true, items } => rsx! {
                        ol { for parts in items { li { InlineView { parts } } } }
                    },
                    Block::List { ordered: false, items } => rsx! {
                        ul { for parts in items { li { InlineView { parts } } } }
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tables() {
        let (c, r) = table_model(&json!([{"a": 1, "b": "x"}, {"b": "y", "c": null}])).unwrap();
        assert_eq!(c, vec!["a", "b", "c"]);
        assert_eq!(r, vec![vec!["1", "x", ""], vec!["", "y", ""]]);
        let (c, r) = table_model(&json!([["name", "n"], ["a", 1]])).unwrap();
        assert_eq!(
            (c, r),
            (
                vec!["name".to_string(), "n".to_string()],
                vec![vec!["a".to_string(), "1".to_string()]]
            )
        );
        let (c, _) = table_model(&json!({"k": 1})).unwrap();
        assert_eq!(c, vec!["key", "value"]);
        assert!(table_model(&json!(3)).is_none());
        assert!(table_model(&json!([])).is_none());
    }

    #[test]
    fn charts() {
        let c = chart_model(&json!({"type": "bar", "labels": ["a", "b"],
            "series": [{"name": "s", "values": [1, 2]}]}))
        .unwrap();
        assert_eq!(c.kind, ChartKind::Bar);
        assert_eq!(c.series[0].1, vec![1.0, 2.0]);
        assert_eq!(
            chart_model(&json!([3, 1, 2])).unwrap().labels,
            vec!["1", "2", "3"]
        );
        let points = chart_model(&json!([{"x": "mon", "y": 2}, {"x": "tue", "y": 3.5}])).unwrap();
        assert_eq!(points.labels, vec!["mon", "tue"]);
        assert_eq!(
            chart_model(&json!({"fr": 1, "de": 2})).unwrap().kind,
            ChartKind::Bar
        );
        assert!(chart_model(&json!("nope")).is_none());
        assert!(chart_model(&json!([{"x": 1}])).is_none());
    }

    #[test]
    fn markdown_has_no_html() {
        let blocks = markdown("# Total <script>alert(1)</script>\n\nSome **bold** and `code`, a [link](https://example.com) \
            and a [trap](javascript:alert(1)).\n\n- one\n- two\n\n1. first\n\n```\n<b>raw</b>\n```\n> quoted\n---");
        assert_eq!(
            blocks[0],
            Block::Heading(
                1,
                vec![Inline::Text("Total <script>alert(1)</script>".into())]
            )
        );
        let Block::Paragraph(p) = &blocks[1] else {
            panic!("{:?}", blocks[1])
        };
        assert!(p.contains(&Inline::Strong("bold".into())));
        assert!(p.contains(&Inline::Code("code".into())));
        assert!(p.contains(&Inline::Link {
            text: "link".into(),
            href: "https://example.com".into()
        }));
        assert!(p
            .iter()
            .all(|i| !matches!(i, Inline::Link { href, .. } if href.starts_with("javascript"))));
        assert!(matches!(&blocks[2], Block::List { ordered: false, items } if items.len() == 2));
        assert!(matches!(&blocks[3], Block::List { ordered: true, .. }));
        assert_eq!(blocks[4], Block::Code("<b>raw</b>".into()));
        assert_eq!(blocks[5], Block::Quote(vec![Inline::Text("quoted".into())]));
        assert_eq!(blocks[6], Block::Rule);
        assert_eq!(safe_href("JAVASCRIPT:x"), None);
        assert_eq!(safe_href("mailto:a@b.c").as_deref(), Some("mailto:a@b.c"));
        assert_eq!(inlines("a * b"), vec![Inline::Text("a * b".into())]);
        assert_eq!(
            inlines("é_x_"),
            vec![Inline::Text("é".into()), Inline::Em("x".into())]
        );
    }
}
