use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag};

#[derive(Clone, Debug, PartialEq)]
pub enum BlockKind {
    Heading(u8),
    Paragraph,
    CodeBlock,
    BlockQuote,
    ListItem,
    MathBlock,
    ThematicBreak,
    Table,
}

impl BlockKind {
    /// Does Enter split this block into two AST blocks?
    pub fn enter_splits(&self) -> bool {
        matches!(
            self,
            BlockKind::Heading(_) | BlockKind::Paragraph | BlockKind::ThematicBreak
        )
    }

    /// Is this block inherently multi-line? (Enter inserts `<br>` within it)
    pub fn is_multiline(&self) -> bool {
        matches!(
            self,
            BlockKind::CodeBlock | BlockKind::MathBlock | BlockKind::BlockQuote | BlockKind::Table
        )
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            BlockKind::Heading(1) => "md-h1",
            BlockKind::Heading(2) => "md-h2",
            BlockKind::Heading(3) => "md-h3",
            BlockKind::Heading(4) => "md-h4",
            BlockKind::Heading(5) => "md-h5",
            BlockKind::Heading(_) => "md-h6",
            BlockKind::Paragraph => "md-p",
            BlockKind::CodeBlock => "md-code",
            BlockKind::BlockQuote => "md-blockquote",
            BlockKind::ListItem => "md-li",
            BlockKind::MathBlock => "md-math-block",
            BlockKind::ThematicBreak => "md-hr",
            BlockKind::Table => "md-table",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MdBlock {
    pub kind: BlockKind,
    pub source_range: Range<usize>,
    pub source: String,
    pub rendered_html: String,
}

/// Detect the list marker from a list-item source line and return the next marker.
/// E.g. `"- foo"` → `"- "`, `"1. foo"` → `"2. "`.
pub fn detect_list_marker(source: &str) -> String {
    let trimmed = source.trim_start();
    let indent = &source[..source.len() - trimmed.len()];
    if trimmed.starts_with("- ") {
        format!("{indent}- ")
    } else if trimmed.starts_with("* ") {
        format!("{indent}* ")
    } else if trimmed.starts_with("+ ") {
        format!("{indent}+ ")
    } else if let Some(dot) = trimmed.find(". ") {
        if let Ok(n) = trimmed[..dot].parse::<u32>() {
            format!("{indent}{}. ", n + 1)
        } else {
            format!("{indent}- ")
        }
    } else {
        format!("{indent}- ")
    }
}

/// Render a markdown block source to HTML, emitting data-math attributes for KaTeX.
fn render_block_html(source: &str, kind: &BlockKind) -> String {
    match kind {
        BlockKind::MathBlock => {
            // Strip $$ delimiters and render as a data-math div
            let inner = source.trim();
            let inner = inner.strip_prefix("$$").unwrap_or(inner);
            let inner = inner.strip_suffix("$$").unwrap_or(inner);
            let inner = inner.trim();
            let escaped = html_escape(inner);
            format!("<div class=\"math-display\" data-math=\"{escaped}\">{escaped}</div>")
        }
        _ => {
            // Use pulldown-cmark for standard rendering
            let opts = parser_options();
            let parser = Parser::new_ext(source, opts);
            let mut html_out = String::new();
            // Custom rendering to handle inline math
            for event in parser {
                match event {
                    Event::InlineMath(text) => {
                        let escaped = html_escape(&text);
                        html_out.push_str(&format!(
                            "<span class=\"math-inline\" data-math=\"{escaped}\">{escaped}</span>"
                        ));
                    }
                    Event::DisplayMath(text) => {
                        let escaped = html_escape(&text);
                        html_out.push_str(&format!(
                            "<div class=\"math-display\" data-math=\"{escaped}\">{escaped}</div>"
                        ));
                    }
                    _ => {
                        // Use pulldown-cmark's HTML push for everything else
                        pulldown_cmark::html::push_html(
                            &mut html_out,
                            std::iter::once(event),
                        );
                    }
                }
            }
            html_out
        }
    }
}

fn parser_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_MATH);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Parse markdown source into blocks with byte ranges.
pub fn parse_blocks(source: &str) -> Vec<MdBlock> {
    let opts = parser_options();
    let parser = Parser::new_ext(source, opts).into_offset_iter();

    let mut blocks: Vec<MdBlock> = Vec::new();
    let mut depth: usize = 0;

    for (event, range) in parser {
        match &event {
            // Top-level block starts
            Event::Start(tag) => {
                if depth == 0 {
                    let kind = match tag {
                        Tag::Heading { level, .. } => {
                            BlockKind::Heading(*level as u8)
                        }
                        Tag::Paragraph => BlockKind::Paragraph,
                        Tag::CodeBlock(_) => BlockKind::CodeBlock,
                        Tag::BlockQuote(_) => BlockKind::BlockQuote,
                        Tag::Item => BlockKind::ListItem,
                        Tag::Table(_) => BlockKind::Table,
                        Tag::List(_) => {
                            // Lists are containers; we track items inside
                            depth += 1;
                            continue;
                        }
                        _ => {
                            depth += 1;
                            continue;
                        }
                    };
                    let src = &source[range.clone()];
                    let html = render_block_html(src, &kind);
                    blocks.push(MdBlock {
                        kind,
                        source_range: range,
                        source: src.to_string(),
                        rendered_html: html,
                    });
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }

            // Display math as standalone block (depth 0)
            Event::DisplayMath(text) if depth == 0 => {
                let kind = BlockKind::MathBlock;
                let src = &source[range.clone()];
                let escaped = html_escape(text);
                let html = format!(
                    "<div class=\"math-display\" data-math=\"{escaped}\">{escaped}</div>"
                );
                blocks.push(MdBlock {
                    kind,
                    source_range: range,
                    source: src.to_string(),
                    rendered_html: html,
                });
            }

            // Thematic break (---, ***, ___)
            Event::Rule if depth == 0 => {
                let src = &source[range.clone()];
                blocks.push(MdBlock {
                    kind: BlockKind::ThematicBreak,
                    source_range: range,
                    source: src.to_string(),
                    rendered_html: "<hr>".to_string(),
                });
            }

            _ => {}
        }
    }

    // If there are no blocks but the source is non-empty, treat as single paragraph
    if blocks.is_empty() && !source.trim().is_empty() {
        blocks.push(MdBlock {
            kind: BlockKind::Paragraph,
            source_range: 0..source.len(),
            source: source.to_string(),
            rendered_html: render_block_html(source, &BlockKind::Paragraph),
        });
    }

    blocks
}

/// Render the raw markdown source for the active block with syntax chars dimmed.
/// Returns HTML where syntax markers are wrapped in `<span class="md-syntax">`.
pub fn render_active_block(block: &MdBlock) -> String {
    let src = &block.source;
    match &block.kind {
        BlockKind::Heading(level) => {
            let prefix_len = *level as usize + 1; // "# " = 2, "## " = 3, etc.
            if src.len() >= prefix_len {
                let (prefix, rest) = src.split_at(prefix_len);
                format!(
                    "<span class=\"md-syntax\">{}</span>{}",
                    html_escape(prefix),
                    render_inline_syntax(rest),
                )
            } else {
                render_inline_syntax(src)
            }
        }
        BlockKind::CodeBlock => {
            // Dim the ``` fences, show content as-is
            let lines: Vec<&str> = src.lines().collect();
            let mut out = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                if line.trim_start().starts_with("```") {
                    out.push_str(&format!(
                        "<span class=\"md-syntax\">{}</span>",
                        html_escape(line)
                    ));
                } else {
                    out.push_str(&html_escape(line));
                }
            }
            out
        }
        BlockKind::BlockQuote => {
            // Dim `> ` prefix on each line
            let lines: Vec<&str> = src.lines().collect();
            let mut out = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                if let Some(rest) = line.strip_prefix("> ") {
                    out.push_str("<span class=\"md-syntax\">&gt; </span>");
                    out.push_str(&render_inline_syntax(rest));
                } else if let Some(rest) = line.strip_prefix(">") {
                    out.push_str("<span class=\"md-syntax\">&gt;</span>");
                    out.push_str(&render_inline_syntax(rest));
                } else {
                    out.push_str(&render_inline_syntax(line));
                }
            }
            out
        }
        BlockKind::ListItem => {
            // Dim the `- `, `* `, or `1. ` prefix
            let trimmed = src.trim_start();
            let indent = src.len() - trimmed.len();
            let indent_str = &src[..indent];

            if let Some(rest) = trimmed.strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("+ "))
            {
                let marker = &trimmed[..2];
                format!(
                    "{}<span class=\"md-syntax\">{}</span>{}",
                    html_escape(indent_str),
                    html_escape(marker),
                    render_inline_syntax(rest),
                )
            } else if let Some(dot_pos) = trimmed.find(". ") {
                // Ordered list: "1. text"
                let marker = &trimmed[..dot_pos + 2];
                let rest = &trimmed[dot_pos + 2..];
                format!(
                    "{}<span class=\"md-syntax\">{}</span>{}",
                    html_escape(indent_str),
                    html_escape(marker),
                    render_inline_syntax(rest),
                )
            } else {
                render_inline_syntax(src)
            }
        }
        BlockKind::MathBlock => {
            // Dim $$ delimiters, show LaTeX source as-is
            let lines: Vec<&str> = src.lines().collect();
            let mut out = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                if line.trim() == "$$" {
                    out.push_str("<span class=\"md-syntax\">$$</span>");
                } else {
                    out.push_str(&html_escape(line));
                }
            }
            out
        }
        _ => render_inline_syntax(src),
    }
}

/// Simple state machine to dim inline syntax markers:
/// **bold**, *italic*, `code`, [links](url), $math$
fn render_inline_syntax(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // ** bold **
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_closing(&chars, i + 2, &['*', '*']) {
                out.push_str("<span class=\"md-syntax\">**</span>");
                let inner: String = chars[i + 2..end].iter().collect();
                out.push_str(&format!("<strong>{}</strong>", html_escape(&inner)));
                out.push_str("<span class=\"md-syntax\">**</span>");
                i = end + 2;
                continue;
            }
        }

        // * italic *
        if chars[i] == '*' && (i + 1 >= len || chars[i + 1] != '*') {
            if let Some(end) = find_closing_single(&chars, i + 1, '*') {
                out.push_str("<span class=\"md-syntax\">*</span>");
                let inner: String = chars[i + 1..end].iter().collect();
                out.push_str(&format!("<em>{}</em>", html_escape(&inner)));
                out.push_str("<span class=\"md-syntax\">*</span>");
                i = end + 1;
                continue;
            }
        }

        // ~~strikethrough~~
        if i + 1 < len && chars[i] == '~' && chars[i + 1] == '~' {
            if let Some(end) = find_closing(&chars, i + 2, &['~', '~']) {
                out.push_str("<span class=\"md-syntax\">~~</span>");
                let inner: String = chars[i + 2..end].iter().collect();
                out.push_str(&format!("<del>{}</del>", html_escape(&inner)));
                out.push_str("<span class=\"md-syntax\">~~</span>");
                i = end + 2;
                continue;
            }
        }

        // `code`
        if chars[i] == '`' {
            if let Some(end) = find_closing_single(&chars, i + 1, '`') {
                out.push_str("<span class=\"md-syntax\">`</span>");
                let inner: String = chars[i + 1..end].iter().collect();
                out.push_str(&format!("<code>{}</code>", html_escape(&inner)));
                out.push_str("<span class=\"md-syntax\">`</span>");
                i = end + 1;
                continue;
            }
        }

        // $inline math$
        if chars[i] == '$' && (i + 1 >= len || chars[i + 1] != '$') {
            if let Some(end) = find_closing_single(&chars, i + 1, '$') {
                out.push_str("<span class=\"md-syntax\">$</span>");
                let inner: String = chars[i + 1..end].iter().collect();
                out.push_str(&html_escape(&inner));
                out.push_str("<span class=\"md-syntax\">$</span>");
                i = end + 1;
                continue;
            }
        }

        // [text](url)
        if chars[i] == '[' {
            if let Some((text_end, url_end)) = find_link(&chars, i) {
                let link_text: String = chars[i + 1..text_end].iter().collect();
                let url: String = chars[text_end + 2..url_end].iter().collect();
                out.push_str("<span class=\"md-syntax\">[</span>");
                out.push_str(&html_escape(&link_text));
                out.push_str("<span class=\"md-syntax\">](</span>");
                out.push_str(&html_escape(&url));
                out.push_str("<span class=\"md-syntax\">)</span>");
                i = url_end + 1;
                continue;
            }
        }

        // Plain character
        let c = chars[i];
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
        i += 1;
    }

    out
}

/// Find closing two-char delimiter (e.g. ** or ~~)
fn find_closing(chars: &[char], start: usize, delim: &[char; 2]) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == delim[0] && chars[i + 1] == delim[1] {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find closing single-char delimiter
fn find_closing_single(chars: &[char], start: usize, delim: char) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == delim {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find [text](url) pattern: returns (text_end_idx, url_end_idx)
fn find_link(chars: &[char], start: usize) -> Option<(usize, usize)> {
    // start points to '['
    let mut i = start + 1;
    // Find closing ]
    while i < chars.len() && chars[i] != ']' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let text_end = i;
    // Expect (
    if i + 1 >= chars.len() || chars[i + 1] != '(' {
        return None;
    }
    i += 2;
    // Find closing )
    while i < chars.len() && chars[i] != ')' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    Some((text_end, i))
}
