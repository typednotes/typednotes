use dioxus::prelude::*;
use serde::Deserialize;

use super::blocks::{detect_list_marker, parse_blocks, render_active_block, BlockKind, MdBlock};

/// Result of the batched DOM read (text + cursor block in one round-trip).
#[derive(Deserialize)]
struct SyncResult {
    /// Text content (block text or full editor text)
    t: String,
    /// Cursor block index (-1 = not found)
    b: i64,
    /// true if fallback (full text, not block-specific)
    f: bool,
    /// Cursor is at start of active block
    s: bool,
    /// Cursor is at end of active block
    e: bool,
}

/// Result of reading cursor position within the active block.
#[derive(Deserialize)]
struct CursorOffset {
    /// Character offset from start of block text
    offset: usize,
    /// Full block text (innerText)
    text: String,
}

const STYLE_CSS: Asset = asset!("./style.css");

/// KaTeX CDN URLs
const KATEX_CSS: &str = "https://cdn.jsdelivr.net/npm/katex@0.16.28/dist/katex.min.css";
const KATEX_JS: &str = "https://cdn.jsdelivr.net/npm/katex@0.16.28/dist/katex.min.js";

/// JS snippet to render all math elements via KaTeX
const RENDER_MATH_JS: &str = r#"
(function() {
    if (typeof katex === 'undefined') return;
    document.querySelectorAll('[data-math]').forEach(function(el) {
        if (el.dataset.rendered === 'true') return;
        try {
            katex.render(el.dataset.math, el, {
                displayMode: el.classList.contains('math-display'),
                throwOnError: false
            });
            el.dataset.rendered = 'true';
        } catch(e) {}
    });
})();
"#;

/// Escape a string so it's safe to embed inside a JS string literal (double-quoted).
fn js_string_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Simple counter for unique IDs
static EDITOR_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Build JS to surgically update individual block divs in the DOM.
/// Deactivates old_active (renders as markdown) and activates new_active (shows raw source).
/// Never touches any block other than these two.
fn surgical_swap_js(
    eid: &str,
    blocks: &[MdBlock],
    old_active: Option<usize>,
    new_active: Option<usize>,
) -> String {
    let mut js = format!(
        "(function() {{ var el = document.getElementById({eid_js}); if (!el) return;\n",
        eid_js = js_string_escape(eid),
    );

    // Deactivate old block: replace innerHTML with rendered markdown
    if let Some(old_idx) = old_active {
        if let Some(block) = blocks.get(old_idx) {
            let kind_class = block.kind.css_class();
            js.push_str(&format!(
                "var ob = el.querySelector('[data-block=\"{old_idx}\"]');\
                 if (ob) {{ ob.innerHTML = {html_js}; ob.className = {cls_js}; }}\n",
                html_js = js_string_escape(&block.rendered_html),
                cls_js = js_string_escape(&format!("md-block md-block-inactive {kind_class}")),
            ));
        }
    }

    // Activate new block: replace innerHTML with raw source (syntax-dimmed)
    if let Some(new_idx) = new_active {
        if let Some(block) = blocks.get(new_idx) {
            let active_html = render_active_block(block);
            let kind_class = block.kind.css_class();
            js.push_str(&format!(
                "var nb = el.querySelector('[data-block=\"{new_idx}\"]');\
                 if (nb) {{ nb.innerHTML = {html_js}; nb.className = {cls_js};\
                 var range = document.createRange(); var sel = window.getSelection();\
                 range.selectNodeContents(nb); range.collapse(false);\
                 sel.removeAllRanges(); sel.addRange(range); }}\n",
                html_js = js_string_escape(&active_html),
                cls_js = js_string_escape(&format!("md-block md-block-active {kind_class}")),
            ));
        }
    }

    js.push_str("})();");
    js
}

/// JS to render KaTeX math elements within a specific block only.
fn render_math_in_block_js(eid: &str, block_idx: usize) -> String {
    format!(
        r#"(function() {{
            if (typeof katex === 'undefined') return;
            var el = document.getElementById({eid_js});
            if (!el) return;
            var b = el.querySelector('[data-block="{block_idx}"]');
            if (!b) return;
            b.querySelectorAll('[data-math]').forEach(function(el) {{
                if (el.dataset.rendered === 'true') return;
                try {{
                    katex.render(el.dataset.math, el, {{
                        displayMode: el.classList.contains('math-display'),
                        throwOnError: false
                    }});
                    el.dataset.rendered = 'true';
                }} catch(e) {{}}
            }});
        }})();"#,
        eid_js = js_string_escape(eid),
    )
}

/// Build JS to surgically update the DOM after an Enter-split.
/// Deactivates `old_idx`, inserts a new active div after it, renumbers all data-block attrs.
fn build_enter_split_js(
    eid: &str,
    blocks: &[MdBlock],
    old_idx: usize,
    new_active: usize,
) -> String {
    let old_block = blocks.get(old_idx);
    let new_block = blocks.get(new_active);

    let (inactive_html, inactive_class) = match old_block {
        Some(b) => (
            b.rendered_html.clone(),
            format!("md-block md-block-inactive {}", b.kind.css_class()),
        ),
        None => (String::new(), "md-block md-block-inactive md-p".to_string()),
    };

    let (active_html, active_class) = match new_block {
        Some(b) => (
            render_active_block(b),
            format!("md-block md-block-active {}", b.kind.css_class()),
        ),
        None => (String::new(), "md-block md-block-active md-p".to_string()),
    };

    format!(
        r#"(function() {{
            var el = document.getElementById({eid_js});
            if (!el) return;
            var ob = el.querySelector('[data-block="{old_idx}"]');
            if (ob) {{ ob.innerHTML = {inactive_html_js}; ob.className = {inactive_cls_js}; }}
            var nb = document.createElement('div');
            nb.className = {active_cls_js};
            nb.innerHTML = {active_html_js};
            if (ob) {{ ob.parentNode.insertBefore(nb, ob.nextSibling); }}
            else {{ el.appendChild(nb); }}
            var all = el.querySelectorAll('[data-block]');
            for (var i = 0; i < all.length; i++) {{ all[i].setAttribute('data-block', i); }}
            var range = document.createRange();
            var sel = window.getSelection();
            if (nb.childNodes.length > 0) {{
                var first = nb.childNodes[0];
                if (first.nodeType === 3) {{ range.setStart(first, 0); }}
                else if (first.childNodes.length > 0) {{ range.setStart(first.childNodes[0], 0); }}
                else {{ range.setStart(first, 0); }}
            }} else {{ range.setStart(nb, 0); }}
            range.collapse(true);
            sel.removeAllRanges();
            sel.addRange(range);
        }})();"#,
        eid_js = js_string_escape(eid),
        inactive_html_js = js_string_escape(&inactive_html),
        inactive_cls_js = js_string_escape(&inactive_class),
        active_html_js = js_string_escape(&active_html),
        active_cls_js = js_string_escape(&active_class),
    )
}

#[component]
pub fn MarkdownEditor(
    mut content: Signal<String>,
    on_change: EventHandler<String>,
    #[props(default)] on_blur: EventHandler<()>,
    #[props(default = "Start writing...".to_string())] placeholder: String,
) -> Element {
    let editor_id = use_signal(|| {
        let n = EDITOR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("md-editor-{n}")
    });

    let mut active_block: Signal<Option<usize>> = use_signal(|| None);
    let mut render_version: Signal<u64> = use_signal(|| 0);
    let mut is_dirty = use_signal(|| false);

    // ── Full render effect: fires on mount and blur-to-idle only ──
    // Uses .peek() on other signals so they are NOT reactive dependencies.
    let ph = placeholder.clone();
    use_effect(move || {
        let _ = render_version(); // sole reactive dependency
        let ph = ph.clone();

        let eid = editor_id.peek().clone();
        let text = content.peek().clone();
        let active = *active_block.peek();
        let blocks = parse_blocks(&text);
        let html = build_blocks_html(&blocks, active);

        // Fire-and-forget: set innerHTML
        let js = format!(
            r#"(function() {{
                var el = document.getElementById({eid_js});
                if (!el) return;
                el.setAttribute("data-placeholder", {ph_js});
                el.innerHTML = {html_js};
            }})();"#,
            eid_js = js_string_escape(&eid),
            ph_js = js_string_escape(&ph),
            html_js = js_string_escape(&html),
        );
        document::eval(&js);

        // Fire-and-forget: render math in inactive blocks
        document::eval(RENDER_MATH_JS);

        // Fire-and-forget: restore cursor to active block if any
        if let Some(idx) = active {
            let restore_js = format!(
                r#"(function() {{
                    var el = document.getElementById({eid_js});
                    if (!el) return;
                    var block = el.querySelector('[data-block="{idx}"]');
                    if (!block) return;
                    var range = document.createRange();
                    var sel = window.getSelection();
                    if (block.childNodes.length > 0) {{
                        range.selectNodeContents(block);
                        range.collapse(false);
                    }} else {{
                        range.setStart(block, 0);
                        range.collapse(true);
                    }}
                    sel.removeAllRanges();
                    sel.addRange(range);
                }})();"#,
                eid_js = js_string_escape(&eid),
            );
            document::eval(&restore_js);
        }
    });

    // ── Backspace guard: prevent browser from merging block divs ──
    {
        let eid = editor_id.peek().clone();
        use_effect(move || {
            let _ = render_version(); // re-attach after full render
            let js = format!(
                r#"(function() {{
                    var el = document.getElementById({eid_js});
                    if (!el || el._bsGuard) return;
                    el._bsGuard = true;
                    el.addEventListener('beforeinput', function(e) {{
                        if (e.inputType !== 'deleteContentBackward') return;
                        var sel = window.getSelection();
                        if (!sel || sel.rangeCount === 0) return;
                        var range = sel.getRangeAt(0);
                        var node = range.startContainer;
                        while (node && !(node.dataset && node.dataset.block !== undefined)) {{
                            node = node.parentNode;
                        }}
                        if (!node) return;
                        var blockIdx = parseInt(node.dataset.block, 10);
                        if (blockIdx === 0) return;
                        var pre = document.createRange();
                        pre.setStart(node, 0);
                        pre.setEnd(range.startContainer, range.startOffset);
                        if (pre.toString().length === 0) {{
                            e.preventDefault();
                        }}
                    }});
                }})();"#,
                eid_js = js_string_escape(&eid),
            );
            document::eval(&js);
        });
    }

    // ── Load KaTeX CSS + JS once ──
    use_effect(|| {
        let js = format!(
            r#"(function() {{
                if (document.getElementById('katex-css')) return;
                var link = document.createElement('link');
                link.id = 'katex-css';
                link.rel = 'stylesheet';
                link.href = '{KATEX_CSS}';
                document.head.appendChild(link);

                var script = document.createElement('script');
                script.id = 'katex-js';
                script.src = '{KATEX_JS}';
                script.onload = function() {{
                    {RENDER_MATH_JS}
                }};
                document.head.appendChild(script);
            }})();"#,
            KATEX_CSS = KATEX_CSS,
            KATEX_JS = KATEX_JS,
            RENDER_MATH_JS = RENDER_MATH_JS,
        );
        document::eval(&js);
    });

    // ── Transition: sync DOM text, detect/force active block, surgical update ──
    //
    // Key principle: the browser DOM is the live buffer for the active block.
    // We never write to the active block's innerHTML during editing.
    // DOM writes only happen to:
    //   - The OLD block (deactivate: raw source → rendered markdown)
    //   - The NEW block (activate: rendered markdown → raw source)
    //   - All blocks on blur (full render, editor going idle)
    //
    // forced_block: None = auto-detect from cursor, Some(block) = force that value
    // arrow_key: if set, enables boundary override (jump to next/prev block at edge)
    // call_blur: if true, calls on_blur after everything completes
    let do_transition =
        move |forced_block: Option<Option<usize>>, arrow_key: Option<Key>, call_blur: bool| {
            spawn(async move {
                let eid = editor_id();
                let old_active = active_block();
                let need_text = is_dirty();

                // ── Blur path: sync text then full render (all blocks inactive) ──
                if forced_block == Some(None) {
                    if need_text {
                        if let Some(idx) = old_active {
                            // Read active block text
                            let js = format!(
                                r#"(function() {{
                                    var el = document.getElementById({eid_js});
                                    if (!el) {{ dioxus.send(""); return; }}
                                    var b = el.querySelector('[data-block="{idx}"]');
                                    dioxus.send(b ? (b.innerText || "") : "");
                                }})();"#,
                                eid_js = js_string_escape(&eid),
                            );
                            let mut eval = document::eval(&js);
                            if let Ok(text) = eval.recv::<String>().await {
                                let blocks_snapshot = parse_blocks(&content());
                                if let Some(block) = blocks_snapshot.get(idx) {
                                    // Normalize: strip trailing \n (browsers add it for trailing <br>)
                                    let text = if !block.kind.is_multiline() {
                                        text.trim_end_matches('\n').to_string()
                                    } else {
                                        text.strip_suffix('\n').unwrap_or(&text).to_string()
                                    };
                                    let mut new_content = content();
                                    let range = block.source_range.clone();
                                    if range.start <= new_content.len()
                                        && range.end <= new_content.len()
                                    {
                                        new_content.replace_range(range, &text);
                                        content.set(new_content.clone());
                                        on_change.call(new_content);
                                    }
                                }
                            }
                        } else {
                            // No active block — read full editor text as fallback
                            let js = format!(
                                r#"(function() {{
                                    var el = document.getElementById({eid_js});
                                    dioxus.send(el ? (el.innerText || "") : "");
                                }})();"#,
                                eid_js = js_string_escape(&eid),
                            );
                            let mut eval = document::eval(&js);
                            if let Ok(text) = eval.recv::<String>().await {
                                if !text.is_empty() {
                                    content.set(text.clone());
                                    on_change.call(text);
                                }
                            }
                        }
                        is_dirty.set(false);
                    }
                    active_block.set(None);
                    render_version += 1;
                    if call_blur {
                        on_blur.call(());
                    }
                    return;
                }

                // ── Non-blur: surgical block transition ──
                let need_cursor = forced_block.is_none();

                // Fast path: forced block, not dirty — just surgical swap if different
                if !need_text && !need_cursor {
                    if let Some(forced) = forced_block {
                        if forced != old_active {
                            active_block.set(forced);
                            let blocks = parse_blocks(&content());
                            document::eval(&surgical_swap_js(&eid, &blocks, old_active, forced));
                            if let Some(old_idx) = old_active {
                                document::eval(&render_math_in_block_js(&eid, old_idx));
                            }
                        }
                    }
                    if call_blur {
                        on_blur.call(());
                    }
                    return;
                }

                // Batched DOM read: text content + cursor block index + position
                let block_idx =
                    old_active.map(|i| i.to_string()).unwrap_or_default();
                let js = format!(
                    r#"(function() {{
                    var el = document.getElementById({eid_js});
                    if (!el) {{ dioxus.send({{t:"",b:-1,f:true,s:false,e:false}}); return; }}

                    var text = null;
                    var bidx = {block_idx_js};
                    if (bidx !== "") {{
                        var blocks = el.querySelectorAll('[data-block="' + bidx + '"]');
                        if (blocks.length > 0) {{
                            var parts = [];
                            blocks.forEach(function(b) {{ parts.push(b.innerText || ""); }});
                            text = parts.join("\n");
                        }}
                    }}

                    var fallback = text === null;
                    if (fallback) {{ text = el.innerText || ""; }}

                    var blockIdx = -1;
                    var sel = window.getSelection();
                    if (sel && sel.rangeCount > 0) {{
                        var node = sel.anchorNode;
                        while (node && node.id !== {eid_js}) {{
                            if (node.dataset && node.dataset.block !== undefined) {{
                                blockIdx = parseInt(node.dataset.block, 10);
                                break;
                            }}
                            node = node.parentNode;
                        }}
                    }}

                    var atStart = false, atEnd = false;
                    if (blockIdx >= 0 && sel && sel.rangeCount > 0) {{
                        var ab = el.querySelector('[data-block="' + blockIdx + '"]');
                        if (ab) {{
                            var r = sel.getRangeAt(0);
                            try {{
                                var pre = document.createRange();
                                pre.setStart(ab, 0);
                                pre.setEnd(r.startContainer, r.startOffset);
                                atStart = pre.toString().length === 0;
                                var post = document.createRange();
                                post.setStart(r.endContainer, r.endOffset);
                                post.setEndAfter(ab.lastChild || ab);
                                atEnd = post.toString().length === 0;
                            }} catch(e) {{}}
                        }}
                    }}

                    dioxus.send({{t: text, b: blockIdx, f: fallback, s: atStart, e: atEnd}});
                }})();"#,
                    eid_js = js_string_escape(&eid),
                    block_idx_js = js_string_escape(&block_idx),
                );

                let mut eval = document::eval(&js);
                if let Ok(data) = eval.recv::<SyncResult>().await {
                    // Sync text if dirty
                    if need_text {
                        if data.f {
                            // Fallback: full editor text
                            if !data.t.is_empty() {
                                content.set(data.t.clone());
                                on_change.call(data.t);
                            }
                        } else {
                            // Splice block text into content
                            if let Some(idx) = old_active {
                                let blocks_snapshot = parse_blocks(&content());
                                if let Some(block) = blocks_snapshot.get(idx) {
                                    // Normalize: strip trailing \n (browsers add it for trailing <br>)
                                    let text = if !block.kind.is_multiline() {
                                        data.t.trim_end_matches('\n').to_string()
                                    } else {
                                        data.t.strip_suffix('\n').unwrap_or(&data.t).to_string()
                                    };
                                    let mut new_content = content();
                                    let range = block.source_range.clone();
                                    if range.start <= new_content.len()
                                        && range.end <= new_content.len()
                                    {
                                        new_content.replace_range(range, &text);
                                        content.set(new_content.clone());
                                        on_change.call(new_content);
                                    }
                                }
                            }
                        }
                        is_dirty.set(false);
                    }

                    // Detect block from the same response
                    let mut new_block = match forced_block {
                        Some(forced) => forced,
                        None => {
                            if data.b >= 0 {
                                Some(data.b as usize)
                            } else {
                                None
                            }
                        }
                    };

                    // Arrow boundary override: force next/prev block when at edge
                    if forced_block.is_none() {
                        if let Some(ref key) = arrow_key {
                            let block_count = parse_blocks(&content()).len();
                            match key {
                                Key::ArrowDown if data.e => {
                                    if let Some(cur) = new_block {
                                        if cur + 1 < block_count {
                                            new_block = Some(cur + 1);
                                        }
                                    }
                                }
                                Key::ArrowUp if data.s => {
                                    if let Some(cur) = new_block {
                                        if cur > 0 {
                                            new_block = Some(cur - 1);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // Block changed → surgical DOM update (only the two affected blocks)
                    if new_block != old_active {
                        active_block.set(new_block);
                        let blocks = parse_blocks(&content());
                        document::eval(&surgical_swap_js(&eid, &blocks, old_active, new_block));
                        // Render KaTeX in the newly deactivated block
                        if let Some(old_idx) = old_active {
                            document::eval(&render_math_in_block_js(&eid, old_idx));
                        }
                    }
                    // Same block: model updated if dirty, DOM untouched (it's the live buffer)
                }

                if call_blur {
                    on_blur.call(());
                }
            });
        };

    // ── Event handlers ──

    // Typing: mark dirty + update active block CSS class via Rust parser
    let on_input = move |_: FormEvent| {
        is_dirty.set(true);

        // Read active block text and update CSS class via Rust parser
        if let Some(idx) = active_block.peek().as_ref() {
            let idx = *idx;
            let eid = editor_id.peek().clone();
            spawn(async move {
                let js = format!(
                    r#"(function() {{
                        var el = document.getElementById({eid_js});
                        if (!el) {{ dioxus.send(""); return; }}
                        var b = el.querySelector('[data-block="{idx}"]');
                        dioxus.send(b ? (b.innerText || "") : "");
                    }})();"#,
                    eid_js = js_string_escape(&eid),
                );
                let mut eval = document::eval(&js);
                if let Ok(text) = eval.recv::<String>().await {
                    let blocks = parse_blocks(&text);
                    let new_class = blocks
                        .first()
                        .map(|b| b.kind.css_class())
                        .unwrap_or("md-p");

                    let update_js = format!(
                        r#"(function() {{
                            var el = document.getElementById({eid_js});
                            if (!el) return;
                            var b = el.querySelector('[data-block="{idx}"]');
                            if (!b) return;
                            var kinds = ['md-h1','md-h2','md-h3','md-h4','md-h5','md-h6',
                                         'md-p','md-code','md-blockquote','md-li','md-math-block','md-hr','md-table'];
                            kinds.forEach(function(k) {{ b.classList.remove(k); }});
                            b.classList.add({cls_js});
                        }})();"#,
                        eid_js = js_string_escape(&eid),
                        cls_js = js_string_escape(new_class),
                    );
                    document::eval(&update_js);
                }
            });
        }
    };

    // Click: sync previous block, detect new block
    let on_click = move |_: MouseEvent| {
        do_transition(None, None, false);
    };

    // Arrow keys: may have moved to a different block
    let on_keyup = move |evt: KeyboardEvent| {
        let key = evt.key();
        match key {
            Key::ArrowUp | Key::ArrowDown => {
                do_transition(None, Some(key), false);
            }
            Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End => {
                do_transition(None, None, false);
            }
            _ => {}
        }
    };

    // First focus: detect initial block
    let on_focus_in = move |_: FocusEvent| {
        if active_block().is_none() {
            do_transition(None, None, false);
        }
    };

    // Blur: sync, force all blocks inactive, notify parent
    let on_focus_out = move |_: FocusEvent| {
        do_transition(Some(None), None, true);
    };

    // Enter: AST-aware — split blocks for paragraphs/headings/lists,
    // insertLineBreak for multiline blocks (code, math, blockquote, table)
    // Tab: insert spaces instead of changing focus
    let on_keydown = move |evt: KeyboardEvent| {
        if evt.key() == Key::Enter {
            let idx = match *active_block.peek() {
                Some(i) => i,
                None => {
                    evt.prevent_default();
                    return;
                }
            };
            let blocks = parse_blocks(&content.peek());
            let block = match blocks.get(idx) {
                Some(b) => b,
                None => {
                    evt.prevent_default();
                    return;
                }
            };

            // Multi-line blocks: browser-native line break within the block
            if block.kind.is_multiline() {
                evt.prevent_default();
                document::eval("document.execCommand('insertLineBreak', false, null);");
                is_dirty.set(true);
                return;
            }

            // Paragraphs, headings, list items, HR: split via model
            evt.prevent_default();
            let kind = block.kind.clone();

            spawn(async move {
                let eid = editor_id();

                // 1. Read cursor offset + text from JS
                let js = format!(
                    r#"(function() {{
                        var el = document.getElementById({eid_js});
                        if (!el) {{ dioxus.send({{offset:0,text:""}}); return; }}
                        var b = el.querySelector('[data-block="{idx}"]');
                        if (!b) {{ dioxus.send({{offset:0,text:""}}); return; }}
                        var sel = window.getSelection();
                        if (!sel || sel.rangeCount === 0) {{
                            dioxus.send({{offset: 0, text: b.innerText || ""}});
                            return;
                        }}
                        var r = sel.getRangeAt(0);
                        var pre = document.createRange();
                        pre.setStart(b, 0);
                        pre.setEnd(r.startContainer, r.startOffset);
                        var offset = pre.toString().length;
                        var text = b.innerText || "";
                        dioxus.send({{offset: offset, text: text}});
                    }})();"#,
                    eid_js = js_string_escape(&eid),
                );
                let mut eval = document::eval(&js);
                let cursor_data: CursorOffset = match eval.recv().await {
                    Ok(d) => d,
                    Err(_) => return,
                };

                // 2. Sync block text into model
                let blocks = parse_blocks(&content());
                let block = match blocks.get(idx) {
                    Some(b) => b,
                    None => return,
                };
                let mut new_content = content();
                let range = block.source_range.clone();

                // Normalize: strip trailing \n from innerText
                let synced_text = cursor_data.text.trim_end_matches('\n').to_string();

                if range.start <= new_content.len() && range.end <= new_content.len() {
                    new_content.replace_range(range.clone(), &synced_text);
                }

                // 3. Compute byte offset for split point
                let byte_offset = synced_text
                    .char_indices()
                    .nth(cursor_data.offset)
                    .map(|(i, _)| i)
                    .unwrap_or(synced_text.len());
                let split_byte = range.start + byte_offset;

                // 4. Insert separator
                let sep = match kind {
                    BlockKind::ListItem => format!("\n{}", detect_list_marker(&synced_text)),
                    _ => "\n\n".to_string(),
                };
                new_content.insert_str(split_byte, &sep);
                content.set(new_content.clone());
                on_change.call(new_content);
                is_dirty.set(false);

                // 5. Re-parse and surgically update DOM
                let new_blocks = parse_blocks(&content());
                let new_active = idx + 1;
                active_block.set(Some(new_active));
                document::eval(&build_enter_split_js(&eid, &new_blocks, idx, new_active));
                document::eval(&render_math_in_block_js(&eid, idx));
            });
        } else if evt.key() == Key::Tab {
            evt.prevent_default();
            document::eval(
                r#"(function() {
                    var sel = window.getSelection();
                    if (!sel || sel.rangeCount === 0) return;
                    var range = sel.getRangeAt(0);
                    range.deleteContents();
                    var text = document.createTextNode("  ");
                    range.insertNode(text);
                    range.setStartAfter(text);
                    range.collapse(true);
                    sel.removeAllRanges();
                    sel.addRange(range);
                })();"#,
            );
        }
    };

    rsx! {
        document::Link { rel: "stylesheet", href: STYLE_CSS }
        div {
            id: "{editor_id}",
            class: "md-editor",
            contenteditable: "true",
            spellcheck: "true",
            onclick: on_click,
            onfocusin: on_focus_in,
            onfocusout: on_focus_out,
            onkeyup: on_keyup,
            onkeydown: on_keydown,
            oninput: on_input,
        }
    }
}

/// Build the full HTML for all blocks.
fn build_blocks_html(blocks: &[MdBlock], active: Option<usize>) -> String {
    if blocks.is_empty() {
        return String::new();
    }

    let mut html = String::new();
    for (i, block) in blocks.iter().enumerate() {
        let is_active = active == Some(i);
        let active_class = if is_active {
            "md-block-active"
        } else {
            "md-block-inactive"
        };
        let kind_class = block.kind.css_class();

        html.push_str(&format!(
            "<div class=\"md-block {active_class} {kind_class}\" data-block=\"{i}\">",
        ));

        if is_active {
            html.push_str(&render_active_block(block));
        } else {
            html.push_str(&block.rendered_html);
        }

        html.push_str("</div>");
    }
    html
}
