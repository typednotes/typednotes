use dioxus::prelude::*;

use super::blocks::{parse_blocks, render_active_block, MdBlock};

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

    // ── Render effect: rebuilds DOM only when render_version is bumped ──
    // Reads content/active_block inside spawn so they are NOT reactive dependencies.
    let ph = placeholder.clone();
    use_effect(move || {
        let _ = render_version(); // sole reactive dependency
        let ph = ph.clone();

        spawn(async move {
            let eid = editor_id();
            let text = content();
            let active = active_block();
            let blocks = parse_blocks(&text);
            let html = build_blocks_html(&blocks, active);

            // Fire-and-forget: set innerHTML (no return value needed)
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
    });

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

    // ── Transition: sync DOM → content, detect/force active block, re-render ──
    // forced_block: None = auto-detect, Some(block) = force that value
    // call_blur: if true, calls on_blur after everything completes
    let do_transition = move |forced_block: Option<Option<usize>>, call_blur: bool| {
        spawn(async move {
            let eid = editor_id();
            let mut needs_render = false;

            // Phase 1: Sync DOM text back into content signal if dirty
            if is_dirty() {
                let old_active = active_block();
                let mut synced = false;

                if let Some(idx) = old_active {
                    // Read ALL divs with this data-block (browser may clone them on Enter)
                    let js = format!(
                        r#"(function() {{
                            var el = document.getElementById({eid_js});
                            if (!el) {{ dioxus.send(null); return; }}
                            var blocks = el.querySelectorAll('[data-block="{idx}"]');
                            if (blocks.length === 0) {{ dioxus.send(null); return; }}
                            var parts = [];
                            blocks.forEach(function(b) {{ parts.push(b.innerText || ""); }});
                            dioxus.send(parts.join("\n"));
                        }})();"#,
                        eid_js = js_string_escape(&eid),
                    );
                    let mut eval = document::eval(&js);
                    if let Ok(text) = eval.recv::<Option<String>>().await {
                        if let Some(text) = text {
                            let blocks_snapshot = parse_blocks(&content());
                            if let Some(block) = blocks_snapshot.get(idx) {
                                let mut new_content = content();
                                let range = block.source_range.clone();
                                if range.start <= new_content.len()
                                    && range.end <= new_content.len()
                                {
                                    new_content.replace_range(range, &text);
                                    content.set(new_content.clone());
                                    on_change.call(new_content);
                                    synced = true;
                                }
                            }
                        }
                    }
                }

                // Fallback: no [data-block] divs (fresh typing), read entire innerText
                if !synced {
                    let js = format!(
                        r#"(function() {{
                            var el = document.getElementById({eid_js});
                            if (!el) {{ dioxus.send(""); return; }}
                            dioxus.send(el.innerText || "");
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
                needs_render = true;
            }

            // Phase 2: Detect which block the cursor is in (or use forced value)
            let new_block = match forced_block {
                Some(forced) => forced,
                None => {
                    let js = format!(
                        r#"(function() {{
                            var sel = window.getSelection();
                            if (!sel || sel.rangeCount === 0) {{ dioxus.send(-1); return; }}
                            var node = sel.anchorNode;
                            while (node && node.id !== {eid_js}) {{
                                if (node.dataset && node.dataset.block !== undefined) {{
                                    dioxus.send(parseInt(node.dataset.block, 10));
                                    return;
                                }}
                                node = node.parentNode;
                            }}
                            dioxus.send(-1);
                        }})();"#,
                        eid_js = js_string_escape(&eid),
                    );
                    let mut eval = document::eval(&js);
                    if let Ok(idx) = eval.recv::<i64>().await {
                        if idx >= 0 {
                            Some(idx as usize)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };

            // Phase 3: Update active block and trigger render if anything changed
            if new_block != active_block() {
                active_block.set(new_block);
                needs_render = true;
            }

            if needs_render {
                render_version += 1;
            }

            if call_blur {
                on_blur.call(());
            }
        });
    };

    // ── Event handlers ──

    // Typing: just mark dirty — the browser owns the DOM during typing
    let on_input = move |_: FormEvent| {
        is_dirty.set(true);
    };

    // Click: sync previous block, detect new block
    let on_click = move |_: MouseEvent| {
        do_transition(None, false);
    };

    // Arrow keys: may have moved to a different block
    let on_keyup = move |evt: KeyboardEvent| {
        if matches!(
            evt.key(),
            Key::ArrowUp
                | Key::ArrowDown
                | Key::ArrowLeft
                | Key::ArrowRight
                | Key::Home
                | Key::End
        ) {
            do_transition(None, false);
        }
    };

    // First focus: detect initial block
    let on_focus_in = move |_: FocusEvent| {
        if active_block().is_none() {
            do_transition(None, false);
        }
    };

    // Blur: sync, force all blocks inactive, notify parent
    let on_focus_out = move |_: FocusEvent| {
        do_transition(Some(None), true);
    };

    // Enter: insert line break instead of letting browser clone block divs
    // Tab: insert spaces instead of changing focus
    let on_keydown = move |evt: KeyboardEvent| {
        if evt.key() == Key::Enter {
            evt.prevent_default();
            // insertLineBreak inserts <br> without creating new block-level elements
            document::eval("document.execCommand('insertLineBreak', false, null);");
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
