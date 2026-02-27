use dioxus::prelude::*;

const STYLE_CSS: Asset = asset!("./style.css");

/// KaTeX CDN URLs
const KATEX_CSS: &str = "https://cdn.jsdelivr.net/npm/katex@0.16.28/dist/katex.min.css";
const KATEX_JS: &str = "https://cdn.jsdelivr.net/npm/katex@0.16.28/dist/katex.min.js";

/// highlight.js CDN URLs
const HLJS_CSS: &str = "https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.11.1/build/styles/github.min.css";
const HLJS_JS: &str = "https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.11.1/build/highlight.min.js";

/// Simple counter for unique IDs
static EDITOR_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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

#[component]
pub fn MarkdownEditor(
    mut content: Signal<String>,
    on_change: EventHandler<String>,
    #[props(default)] on_blur: EventHandler<()>,
    #[props(default = "Start writing...".to_string())] placeholder: String,
) -> Element {
    let editor_id = use_signal(|| {
        let n = EDITOR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("cm-editor-{n}")
    });

    // Track whether we initialized the CM6 instance
    let mut initialized = use_signal(|| false);
    // Track latest content to avoid echo loops
    let mut last_pushed = use_signal(|| String::new());

    // ── Load KaTeX + highlight.js CSS + JS once ──
    use_effect(|| {
        let js = format!(
            r#"(function() {{
                if (!document.getElementById('katex-css')) {{
                    var link = document.createElement('link');
                    link.id = 'katex-css';
                    link.rel = 'stylesheet';
                    link.href = '{KATEX_CSS}';
                    document.head.appendChild(link);
                    var script = document.createElement('script');
                    script.id = 'katex-js';
                    script.src = '{KATEX_JS}';
                    document.head.appendChild(script);
                }}
                if (!document.getElementById('hljs-css')) {{
                    var link2 = document.createElement('link');
                    link2.id = 'hljs-css';
                    link2.rel = 'stylesheet';
                    link2.href = '{HLJS_CSS}';
                    document.head.appendChild(link2);
                    var script2 = document.createElement('script');
                    script2.id = 'hljs-js';
                    script2.src = '{HLJS_JS}';
                    document.head.appendChild(script2);
                }}
            }})();"#,
        );
        document::eval(&js);
    });

    // ── Load CM6 bundle script once ──
    use_effect(move || {
        let js = r#"(function() {
            if (document.getElementById('cm6-bundle')) { dioxus.send(true); return; }
            var script = document.createElement('script');
            script.id = 'cm6-bundle';
            script.src = '/codemirror-md.js';
            script.onload = function() { dioxus.send(true); };
            script.onerror = function() { dioxus.send(false); };
            document.head.appendChild(script);
        })();"#;
        spawn(async move {
            let mut eval = document::eval(js);
            if let Ok(true) = eval.recv::<bool>().await {
                initialized.set(true);
            }
        });
    });

    // ── Initialize CM6 editor once bundle is loaded ──
    {
        let ph = placeholder.clone();
        use_effect(move || {
            if !initialized() {
                return;
            }

            let eid = editor_id.peek().clone();
            let text = content.peek().clone();
            last_pushed.set(text.clone());

            // Create the editor with event bridge
            let js = format!(
                r#"(function() {{
                    var container = document.getElementById({eid_js});
                    if (!container || container._cm) return;
                    if (typeof TypedNotesCM === 'undefined') return;

                    var editor = TypedNotesCM.createEditor(container, {{
                        content: {content_js},
                        placeholder: {ph_js},
                        onChange: function(text) {{
                            container._cmLastContent = text;
                            if (container._cmChangeTimer) clearTimeout(container._cmChangeTimer);
                            container._cmChangeTimer = setTimeout(function() {{
                                if (container._cmOnChange) container._cmOnChange(text);
                            }}, 50);
                        }},
                        onBlur: function() {{
                            if (container._cmOnBlur) container._cmOnBlur();
                        }}
                    }});
                    container._cm = editor;
                }})();"#,
                eid_js = js_string_escape(&eid),
                content_js = js_string_escape(&text),
                ph_js = js_string_escape(&ph),
            );
            document::eval(&js);

            // Set up the event bridge: long-lived eval that receives change events
            let eid2 = eid.clone();
            spawn(async move {
                // onChange bridge
                let bridge_js = format!(
                    r#"(function() {{
                        var container = document.getElementById({eid_js});
                        if (!container) return;
                        container._cmOnChange = function(text) {{
                            dioxus.send(text);
                        }};
                        // Send any content that accumulated before bridge was ready
                        if (container._cmLastContent !== undefined) {{
                            dioxus.send(container._cmLastContent);
                        }}
                    }})();"#,
                    eid_js = js_string_escape(&eid2),
                );
                let mut eval = document::eval(&bridge_js);
                loop {
                    match eval.recv::<String>().await {
                        Ok(text) => {
                            last_pushed.set(text.clone());
                            content.set(text.clone());
                            on_change.call(text);
                        }
                        Err(_) => break,
                    }
                }
            });

            // onBlur bridge
            let eid3 = eid.clone();
            spawn(async move {
                let bridge_js = format!(
                    r#"(function() {{
                        var container = document.getElementById({eid_js});
                        if (!container) return;
                        container._cmOnBlur = function() {{
                            dioxus.send(true);
                        }};
                    }})();"#,
                    eid_js = js_string_escape(&eid3),
                );
                let mut eval = document::eval(&bridge_js);
                loop {
                    match eval.recv::<bool>().await {
                        Ok(_) => {
                            on_blur.call(());
                        }
                        Err(_) => break,
                    }
                }
            });
        });
    }

    // ── Sync external content changes into CM6 ──
    // When content signal changes from outside (e.g., navigating to another note),
    // push the new content into the CM6 editor.
    use_effect(move || {
        let text = content();
        if !initialized() {
            return;
        }
        // Skip if this change originated from CM6 itself
        if text == last_pushed() {
            return;
        }
        last_pushed.set(text.clone());

        let eid = editor_id.peek().clone();
        let js = format!(
            r#"(function() {{
                var container = document.getElementById({eid_js});
                if (!container || !container._cm) return;
                container._cm.setContent({text_js});
            }})();"#,
            eid_js = js_string_escape(&eid),
            text_js = js_string_escape(&text),
        );
        document::eval(&js);
    });

    rsx! {
        document::Link { rel: "stylesheet", href: STYLE_CSS }
        div {
            id: "{editor_id}",
            class: "cm-wrapper",
        }
    }
}
