import { EditorView } from "@codemirror/view";

// Theme that matches the TypedNotes app CSS variables
export const typedNotesTheme = EditorView.theme({
  "&": {
    color: "var(--secondary-color-4)",
    backgroundColor: "transparent",
    fontFamily: "inherit",
    fontSize: "1rem",
    lineHeight: "1.7",
  },
  "&.cm-focused": {
    outline: "none",
  },
  ".cm-scroller": {
    fontFamily: "inherit",
  },
  ".cm-content": {
    caretColor: "var(--secondary-color-4)",
    padding: "0",
    whiteSpace: "pre-wrap",
    wordWrap: "break-word",
  },
  ".cm-line": {
    padding: "0",
  },
  ".cm-cursor": {
    borderLeftColor: "var(--secondary-color-4)",
  },
  ".cm-selectionBackground": {
    backgroundColor: "var(--primary-color-5, rgba(59,130,246,0.3)) !important",
  },
  "&.cm-focused .cm-selectionBackground": {
    backgroundColor: "var(--primary-color-5, rgba(59,130,246,0.3)) !important",
  },
  ".cm-activeLine": {
    backgroundColor: "transparent",
  },
  ".cm-gutters": {
    display: "none",
  },
  // Placeholder styling
  ".cm-placeholder": {
    color: "var(--secondary-color-5)",
    fontStyle: "normal",
  },

  // Markdown formatting: headings
  ".cm-md-h1": { fontSize: "1.875rem", fontWeight: "700", lineHeight: "1.3" },
  ".cm-md-h2": { fontSize: "1.5rem", fontWeight: "650", lineHeight: "1.35" },
  ".cm-md-h3": { fontSize: "1.25rem", fontWeight: "600", lineHeight: "1.4" },
  ".cm-md-h4": { fontSize: "1.125rem", fontWeight: "600", lineHeight: "1.45" },
  ".cm-md-h5": { fontSize: "1rem", fontWeight: "600", lineHeight: "1.5" },
  ".cm-md-h6": { fontSize: "0.875rem", fontWeight: "600", lineHeight: "1.5" },

  // Inline formatting
  ".cm-md-bold": { fontWeight: "700" },
  ".cm-md-italic": { fontStyle: "italic" },
  ".cm-md-strikethrough": { textDecoration: "line-through" },
  ".cm-md-code": {
    fontFamily: 'ui-monospace, "SF Mono", "Cascadia Code", Menlo, Consolas, monospace',
    fontSize: "0.875em",
    backgroundColor: "var(--primary-color-3)",
    padding: "0.1em 0.3em",
    borderRadius: "3px",
  },

  // Code block
  ".cm-md-codeblock": {
    fontFamily: 'ui-monospace, "SF Mono", "Cascadia Code", Menlo, Consolas, monospace',
    fontSize: "0.875rem",
    backgroundColor: "var(--primary-color-3)",
    borderRadius: "0",
    padding: "0 1em",
  },
  ".cm-md-codeblock-first": {
    borderRadius: "6px 6px 0 0",
    paddingTop: "0.75em",
  },
  ".cm-md-codeblock-last": {
    borderRadius: "0 0 6px 6px",
    paddingBottom: "0.75em",
  },
  ".cm-md-codeblock-only": {
    borderRadius: "6px",
    paddingTop: "0.75em",
    paddingBottom: "0.75em",
  },

  // Block quote
  ".cm-md-blockquote": {
    borderLeft: "3px solid var(--secondary-color-6, var(--secondary-color-5))",
    paddingLeft: "1em",
    color: "var(--secondary-color-5)",
    fontStyle: "italic",
  },

  // List item
  ".cm-md-list-item": {
    paddingLeft: "0",
  },

  // Math block
  ".cm-md-math-block": {
    textAlign: "center",
    margin: "0.75em 0",
    padding: "0.5em 0",
  },

  // HR
  ".cm-md-hr": {
    margin: "1em 0",
  },

  // Links
  ".cm-md-link-text": {
    color: "var(--accent-color, #3b82f6)",
    textDecoration: "underline",
  },
  ".cm-md-link-url": {
    color: "var(--secondary-color-6, var(--secondary-color-5))",
    fontSize: "0.9em",
  },

  // Syntax markers (dimmed when visible near cursor)
  ".cm-md-syntax": {
    color: "var(--secondary-color-6, var(--secondary-color-5))",
    fontWeight: "400",
    fontStyle: "normal",
  },

  // Highlight.js widget
  ".cm-hljs-widget": {
    fontFamily: 'ui-monospace, "SF Mono", "Cascadia Code", Menlo, Consolas, monospace',
    fontSize: "0.875em",
  },
  ".cm-hljs-block": {
    backgroundColor: "var(--primary-color-3)",
    borderRadius: "6px",
    padding: "0.75em 1em",
    margin: "0.5em 0",
    display: "block",
    overflowX: "auto",
  },
  ".cm-hljs-inline": {
    backgroundColor: "var(--primary-color-3)",
    padding: "0.1em 0.3em",
    borderRadius: "3px",
  },

  // KaTeX widget
  ".cm-katex-widget": {
    display: "inline-block",
  },
  ".cm-katex-widget .katex-display": {
    margin: "0",
  },
  ".cm-katex-widget.cm-katex-inline .katex": {
    fontSize: "1.05em",
  },
});
