import { EditorState } from "@codemirror/state";
import { EditorView, placeholder as cmPlaceholder, keymap } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { Strikethrough, Table } from "@lezer/markdown";

// Custom InlineMath parser extension for $...$ syntax
const InlineMathExtension = {
  defineNodes: ["InlineMath"],
  parseInline: [{
    name: "InlineMath",
    parse(cx, next, pos) {
      if (next !== 36) return -1; // not $
      // Skip $$ (display math)
      if (pos + 1 < cx.end && cx.char(pos + 1) === 36) return -1;
      // Don't start after another $
      if (pos > cx.offset && cx.char(pos - 1) === 36) return -1;
      let end = pos + 1;
      while (end < cx.end) {
        const ch = cx.char(end);
        if (ch === 36) { // closing $
          // Must have content between delimiters
          if (end > pos + 1) {
            cx.addElement(cx.elt("InlineMath", pos, end + 1));
            return end + 1;
          }
          return -1;
        }
        if (ch === 10) return -1; // no newlines in inline math
        end++;
      }
      return -1;
    }
  }]
};
import { typedNotesTheme } from "./theme.js";
import { livePreviewPlugin } from "./live-preview.js";

/**
 * Create a CodeMirror 6 editor instance inside the given container.
 *
 * @param {HTMLElement} container - DOM element to mount the editor in
 * @param {Object} options
 * @param {string} options.content - Initial markdown content
 * @param {string} options.placeholder - Placeholder text
 * @param {function} options.onChange - Called with (content: string) on doc changes
 * @param {function} options.onBlur - Called when editor loses focus
 * @returns {{ view: EditorView, setContent: (s: string) => void, getContent: () => string, destroy: () => void }}
 */
function createEditor(container, options = {}) {
  const {
    content = "",
    placeholder = "Start writing...",
    onChange = null,
    onBlur = null,
  } = options;

  const extensions = [
    // Core
    history(),
    keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),

    // Markdown language with GFM extensions
    markdown({
      base: markdownLanguage,
      extensions: [Strikethrough, Table, InlineMathExtension],
    }),

    // Our theme
    typedNotesTheme,

    // Live preview (formatting + syntax dimming)
    livePreviewPlugin,

    // Placeholder
    cmPlaceholder(placeholder),

    // Soft-wrap lines
    EditorView.lineWrapping,

    // Tab inserts spaces (2 spaces)
    EditorState.tabSize.of(2),
  ];

  // onChange listener
  if (onChange) {
    extensions.push(
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChange(update.state.doc.toString());
        }
      })
    );
  }

  // onBlur listener
  if (onBlur) {
    extensions.push(
      EditorView.domEventHandlers({
        blur: () => {
          onBlur();
        },
      })
    );
  }

  const state = EditorState.create({
    doc: content,
    extensions,
  });

  const view = new EditorView({
    state,
    parent: container,
  });

  return {
    view,
    setContent(newContent) {
      const current = view.state.doc.toString();
      if (current !== newContent) {
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: newContent },
        });
      }
    },
    getContent() {
      return view.state.doc.toString();
    },
    focus() {
      view.focus();
    },
    destroy() {
      view.destroy();
    },
  };
}

// Expose globally
window.TypedNotesCM = { createEditor };
