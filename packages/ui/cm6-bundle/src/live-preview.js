import {
  Decoration,
  EditorView,
  WidgetType,
} from "@codemirror/view";
import { RangeSetBuilder, StateField } from "@codemirror/state";
import { syntaxTree } from "@codemirror/language";
import { KatexWidget } from "./katex-widget.js";
import { HighlightWidget } from "./highlight-widget.js";
import { TableWidget } from "./table-widget.js";
import { FrontmatterWidget } from "./frontmatter-widget.js";

/**
 * CM6 StateField for live markdown preview.
 *
 * Uses a StateField (not ViewPlugin) so that Decoration.replace() can span
 * across line breaks — required for replacing multi-line FencedCode and
 * display-math blocks with rendered widgets.
 *
 * Algorithm:
 *   For each markdown AST node with syntax characters:
 *     1. Check if cursor overlaps node's source range
 *     2. Identify syntax ranges (HeaderMark, EmphasisMark, CodeMark, etc.)
 *     3. Identify content ranges (text between syntax markers)
 *
 *     If cursor INSIDE node:
 *       - Syntax ranges -> Decoration.mark({ class: "cm-md-syntax" })  [dimmed gray]
 *       - Content ranges -> Decoration.mark({ class: "cm-md-bold" })   [formatted]
 *
 *     If cursor OUTSIDE node:
 *       - Syntax ranges -> Decoration.replace({})                       [hidden]
 *       - Content ranges -> Decoration.mark({ class: "cm-md-bold" })   [formatted]
 *       - Math nodes -> Decoration.replace({ widget: KatexWidget })     [rendered]
 *       - Code nodes -> Decoration.replace({ widget: HighlightWidget }) [rendered]
 */

// Heading level to class map
const HEADING_CLASSES = {
  ATXHeading1: "cm-md-h1",
  ATXHeading2: "cm-md-h2",
  ATXHeading3: "cm-md-h3",
  ATXHeading4: "cm-md-h4",
  ATXHeading5: "cm-md-h5",
  ATXHeading6: "cm-md-h6",
};

/**
 * Check if any cursor (selection head) is inside the given range [from, to].
 */
function cursorInRange(state, from, to) {
  for (const range of state.selection.ranges) {
    const head = range.head;
    if (head >= from && head <= to) return true;
  }
  return false;
}

/**
 * Check if cursor is on the same line as any part of [from, to].
 */
function cursorOnSameLine(state, from, to) {
  const doc = state.doc;
  for (const range of state.selection.ranges) {
    const cursorLine = doc.lineAt(range.head).number;
    const fromLine = doc.lineAt(from).number;
    const toLine = doc.lineAt(Math.min(to, doc.length)).number;
    if (cursorLine >= fromLine && cursorLine <= toLine) return true;
  }
  return false;
}

function buildDecorations(state) {
  const builder = new RangeSetBuilder();
  const tree = syntaxTree(state);

  // Collect all decorations in an array, then sort by from position
  const decos = [];

  // Detect frontmatter range before tree iteration so we can skip
  // HorizontalRule nodes that fall inside it (the first --- is parsed
  // as a thematic break by the Lezer Markdown parser).
  const docText = state.doc.toString();
  const fmRegex = /^---\n([\s\S]*?)\n---/;
  const fmMatch = docText.match(fmRegex);
  const fmRange = fmMatch ? { from: 0, to: fmMatch[0].length } : null;

  tree.iterate({
    enter(node) {
      const { name, from, to } = node;

      // --- ATX Headings ---
      if (name in HEADING_CLASSES) {
        const cls = HEADING_CLASSES[name];
        const cursorInside = cursorOnSameLine(state, from, to);

        // Apply heading class to the whole line(s)
        decos.push({ from, to, deco: Decoration.mark({ class: cls }) });

        // Find HeaderMark children (the # symbols + space)
        node.node.cursor().iterate((child) => {
          if (child.name === "HeaderMark") {
            if (cursorInside) {
              decos.push({
                from: child.from,
                to: child.to,
                deco: Decoration.mark({ class: "cm-md-syntax" }),
              });
            } else {
              // Hide the "# " prefix: HeaderMark + the space after
              let hideEnd = child.to;
              if (hideEnd < state.doc.length) {
                const next = state.doc.sliceString(hideEnd, hideEnd + 1);
                if (next === " ") hideEnd++;
              }
              decos.push({
                from: child.from,
                to: hideEnd,
                deco: Decoration.replace({}),
              });
            }
          }
        });
        return false; // don't recurse further
      }

      // --- Emphasis (italic) ---
      if (name === "Emphasis") {
        const cursorInside = cursorInRange(state, from, to);
        const marks = [];
        node.node.cursor().iterate((child) => {
          if (child.name === "EmphasisMark") {
            marks.push({ from: child.from, to: child.to });
          }
        });

        if (marks.length >= 2) {
          const first = marks[0];
          const last = marks[marks.length - 1];
          decos.push({
            from: first.to,
            to: last.from,
            deco: Decoration.mark({ class: "cm-md-italic" }),
          });
          if (cursorInside) {
            decos.push({ from: first.from, to: first.to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
            decos.push({ from: last.from, to: last.to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
          } else {
            decos.push({ from: first.from, to: first.to, deco: Decoration.replace({}) });
            decos.push({ from: last.from, to: last.to, deco: Decoration.replace({}) });
          }
        }
        return false;
      }

      // --- StrongEmphasis (bold) ---
      if (name === "StrongEmphasis") {
        const cursorInside = cursorInRange(state, from, to);
        const marks = [];
        node.node.cursor().iterate((child) => {
          if (child.name === "EmphasisMark") {
            marks.push({ from: child.from, to: child.to });
          }
        });

        if (marks.length >= 2) {
          const first = marks[0];
          const last = marks[marks.length - 1];
          decos.push({
            from: first.to,
            to: last.from,
            deco: Decoration.mark({ class: "cm-md-bold" }),
          });
          if (cursorInside) {
            decos.push({ from: first.from, to: first.to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
            decos.push({ from: last.from, to: last.to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
          } else {
            decos.push({ from: first.from, to: first.to, deco: Decoration.replace({}) });
            decos.push({ from: last.from, to: last.to, deco: Decoration.replace({}) });
          }
        }
        return false;
      }

      // --- Strikethrough ---
      if (name === "Strikethrough") {
        const cursorInside = cursorInRange(state, from, to);
        const marks = [];
        node.node.cursor().iterate((child) => {
          if (child.name === "StrikethroughMark") {
            marks.push({ from: child.from, to: child.to });
          }
        });

        if (marks.length >= 2) {
          const first = marks[0];
          const last = marks[marks.length - 1];
          decos.push({
            from: first.to,
            to: last.from,
            deco: Decoration.mark({ class: "cm-md-strikethrough" }),
          });
          if (cursorInside) {
            decos.push({ from: first.from, to: first.to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
            decos.push({ from: last.from, to: last.to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
          } else {
            decos.push({ from: first.from, to: first.to, deco: Decoration.replace({}) });
            decos.push({ from: last.from, to: last.to, deco: Decoration.replace({}) });
          }
        }
        return false;
      }

      // --- InlineCode ---
      if (name === "InlineCode") {
        const cursorInside = cursorInRange(state, from, to);
        const marks = [];
        node.node.cursor().iterate((child) => {
          if (child.name === "CodeMark") {
            marks.push({ from: child.from, to: child.to });
          }
        });

        if (marks.length >= 2) {
          const first = marks[0];
          const last = marks[marks.length - 1];
          if (cursorInside) {
            decos.push({
              from: first.to,
              to: last.from,
              deco: Decoration.mark({ class: "cm-md-code" }),
            });
            decos.push({ from: first.from, to: first.to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
            decos.push({ from: last.from, to: last.to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
          } else {
            const code = state.doc.sliceString(first.to, last.from);
            decos.push({
              from,
              to,
              deco: Decoration.replace({ widget: new HighlightWidget(code, null, false) }),
            });
          }
        }
        return false;
      }

      // --- FencedCode ---
      if (name === "FencedCode") {
        const cursorInside = cursorOnSameLine(state, from, to);

        if (!cursorInside) {
          // Extract language from CodeInfo child
          let language = null;
          node.node.cursor().iterate((child) => {
            if (child.name === "CodeInfo") {
              language = state.doc.sliceString(child.from, child.to).trim() || null;
            }
          });

          // Extract code content (between fence lines)
          const fenceMarks = [];
          node.node.cursor().iterate((child) => {
            if (child.name === "CodeMark") {
              fenceMarks.push({ from: child.from, to: child.to });
            }
          });

          if (fenceMarks.length >= 2) {
            const firstFenceLine = state.doc.lineAt(fenceMarks[0].from);
            const lastFenceLine = state.doc.lineAt(fenceMarks[fenceMarks.length - 1].from);
            const codeFrom = firstFenceLine.to + 1;
            const codeTo = lastFenceLine.from;

            let code = "";
            if (codeFrom < codeTo) {
              code = state.doc.sliceString(codeFrom, codeTo);
              if (code.endsWith("\n")) code = code.slice(0, -1);
            }

            // Replace entire FencedCode node with highlighted widget
            decos.push({
              from,
              to,
              deco: Decoration.replace({ widget: new HighlightWidget(code, language, true) }),
            });
          }
        } else {
          // Cursor inside: show raw code with line styling and dimmed fences
          const startLine = state.doc.lineAt(from).number;
          const endLine = state.doc.lineAt(Math.min(to, state.doc.length)).number;
          const totalLines = endLine - startLine + 1;

          for (let ln = startLine; ln <= endLine; ln++) {
            const line = state.doc.line(ln);
            let cls = "cm-md-codeblock";
            if (totalLines === 1) cls += " cm-md-codeblock-only";
            else if (ln === startLine) cls += " cm-md-codeblock-first";
            else if (ln === endLine) cls += " cm-md-codeblock-last";
            decos.push({
              from: line.from,
              to: line.from,
              deco: Decoration.line({ class: cls }),
            });
          }

          node.node.cursor().iterate((child) => {
            if (child.name === "CodeMark") {
              decos.push({
                from: child.from,
                to: child.to,
                deco: Decoration.mark({ class: "cm-md-syntax" }),
              });
            }
          });
        }
        return false;
      }

      // --- Blockquote ---
      if (name === "Blockquote") {
        const cursorInside = cursorOnSameLine(state, from, to);
        const startLine = state.doc.lineAt(from).number;
        const endLine = state.doc.lineAt(Math.min(to, state.doc.length)).number;
        for (let ln = startLine; ln <= endLine; ln++) {
          const line = state.doc.line(ln);
          decos.push({
            from: line.from,
            to: line.from,
            deco: Decoration.line({ class: "cm-md-blockquote" }),
          });
        }

        node.node.cursor().iterate((child) => {
          if (child.name === "QuoteMark") {
            if (cursorInside) {
              decos.push({
                from: child.from,
                to: child.to,
                deco: Decoration.mark({ class: "cm-md-syntax" }),
              });
            } else {
              let hideEnd = child.to;
              if (hideEnd < state.doc.length) {
                const next = state.doc.sliceString(hideEnd, hideEnd + 1);
                if (next === " ") hideEnd++;
              }
              decos.push({
                from: child.from,
                to: hideEnd,
                deco: Decoration.replace({}),
              });
            }
          }
        });
        return;
      }

      // --- Links ---
      if (name === "Link") {
        const cursorInside = cursorInRange(state, from, to);
        if (cursorInside) {
          node.node.cursor().iterate((child) => {
            if (child.name === "LinkMark") {
              decos.push({
                from: child.from,
                to: child.to,
                deco: Decoration.mark({ class: "cm-md-syntax" }),
              });
            } else if (child.name === "URL") {
              decos.push({
                from: child.from,
                to: child.to,
                deco: Decoration.mark({ class: "cm-md-link-url" }),
              });
            }
          });
          let labelStart = null, labelEnd = null;
          node.node.cursor().iterate((child) => {
            if (child.name === "LinkMark" && labelStart === null) {
              labelStart = child.to;
            } else if (child.name === "LinkMark" && labelStart !== null && labelEnd === null) {
              labelEnd = child.from;
            }
          });
          if (labelStart !== null && labelEnd !== null) {
            decos.push({
              from: labelStart,
              to: labelEnd,
              deco: Decoration.mark({ class: "cm-md-link-text" }),
            });
          }
        } else {
          let labelStart = null, labelEnd = null;
          const linkMarks = [];
          node.node.cursor().iterate((child) => {
            if (child.name === "LinkMark") {
              linkMarks.push({ from: child.from, to: child.to });
            }
          });
          if (linkMarks.length >= 2) {
            labelStart = linkMarks[0].to;
            labelEnd = linkMarks[1].from;
          }
          if (labelStart !== null && labelEnd !== null) {
            decos.push({
              from: labelStart,
              to: labelEnd,
              deco: Decoration.mark({ class: "cm-md-link-text" }),
            });
            decos.push({
              from: linkMarks[0].from,
              to: linkMarks[0].to,
              deco: Decoration.replace({}),
            });
            if (linkMarks.length >= 2) {
              decos.push({
                from: linkMarks[1].from,
                to,
                deco: Decoration.replace({}),
              });
            }
          }
        }
        return false;
      }

      // --- HorizontalRule (ThematicBreak) ---
      if (name === "HorizontalRule") {
        // Skip if inside frontmatter range (first --- is not a real HR)
        if (fmRange && from >= fmRange.from && to <= fmRange.to) {
          return false;
        }
        const cursorInside = cursorOnSameLine(state, from, to);
        if (cursorInside) {
          decos.push({
            from,
            to,
            deco: Decoration.mark({ class: "cm-md-syntax" }),
          });
        } else {
          decos.push({
            from,
            to,
            deco: Decoration.replace({ widget: new HRWidget() }),
          });
        }
        return false;
      }

      // --- ListItem ---
      if (name === "ListItem") {
        const cursorInside = cursorOnSameLine(state, from, to);
        node.node.cursor().iterate((child) => {
          if (child.name === "ListMark") {
            if (cursorInside) {
              decos.push({
                from: child.from,
                to: child.to,
                deco: Decoration.mark({ class: "cm-md-syntax" }),
              });
            }
          }
        });
        return;
      }

      // --- Table (GFM) ---
      if (name === "Table") {
        const cursorInside = cursorOnSameLine(state, from, to);
        if (!cursorInside) {
          const text = state.doc.sliceString(from, to);
          decos.push({
            from,
            to,
            deco: Decoration.replace({ widget: new TableWidget(text) }),
          });
        } else {
          // Cursor inside: show raw with dimmed pipes
          const startLine = state.doc.lineAt(from).number;
          const endLine = state.doc.lineAt(Math.min(to, state.doc.length)).number;
          for (let ln = startLine; ln <= endLine; ln++) {
            const line = state.doc.line(ln);
            decos.push({
              from: line.from,
              to: line.from,
              deco: Decoration.line({ class: "cm-md-table-raw" }),
            });
          }
        }
        return false;
      }

      // --- Inline Math ($...$) ---
      if (name === "InlineMath") {
        const cursorInside = cursorInRange(state, from, to);
        const text = state.doc.sliceString(from, to);

        if (cursorInside) {
          decos.push({ from, to: from + 1, deco: Decoration.mark({ class: "cm-md-syntax" }) });
          decos.push({ from: to - 1, to, deco: Decoration.mark({ class: "cm-md-syntax" }) });
        } else {
          const latex = text.slice(1, -1);
          if (latex.trim()) {
            decos.push({
              from,
              to,
              deco: Decoration.replace({ widget: new KatexWidget(latex, false) }),
            });
          }
        }
        return false;
      }
    },
  });

  // Also handle display math blocks ($$...$$) that may not be in the syntax tree
  const doc = state.doc;
  const text = docText;
  const mathBlockRegex = /^\$\$\s*\n([\s\S]*?)\n\$\$\s*$/gm;
  let match;
  while ((match = mathBlockRegex.exec(text)) !== null) {
    const blockFrom = match.index;
    const blockTo = match.index + match[0].length;
    const latex = match[1];

    let handled = false;
    for (const d of decos) {
      if (d.from <= blockFrom && d.to >= blockTo) {
        handled = true;
        break;
      }
    }
    if (handled) continue;

    const cursorInside = cursorOnSameLine(state, blockFrom, blockTo);
    if (cursorInside) {
      const firstLine = doc.lineAt(blockFrom);
      const lastLine = doc.lineAt(blockTo);
      decos.push({
        from: firstLine.from,
        to: firstLine.to,
        deco: Decoration.mark({ class: "cm-md-syntax" }),
      });
      if (firstLine.number !== lastLine.number) {
        decos.push({
          from: lastLine.from,
          to: lastLine.to,
          deco: Decoration.mark({ class: "cm-md-syntax" }),
        });
      }
    } else {
      if (latex.trim()) {
        decos.push({
          from: blockFrom,
          to: blockTo,
          deco: Decoration.replace({ widget: new KatexWidget(latex.trim(), true) }),
        });
      }
    }
  }

  // Handle YAML frontmatter at document start (---\n...\n---)
  // Reuse fmMatch/fmRange detected before tree iteration
  if (fmMatch) {
    const fmFrom = fmRange.from;
    const fmTo = fmRange.to;
    const yaml = fmMatch[1];

    const cursorInside = cursorOnSameLine(state, fmFrom, fmTo);
    if (cursorInside) {
      // Show raw YAML with dimmed --- delimiters
      const firstLine = doc.lineAt(fmFrom);
      const lastLine = doc.lineAt(fmTo);
      decos.push({
        from: firstLine.from,
        to: firstLine.to,
        deco: Decoration.mark({ class: "cm-md-syntax" }),
      });
      if (firstLine.number !== lastLine.number) {
        decos.push({
          from: lastLine.from,
          to: lastLine.to,
          deco: Decoration.mark({ class: "cm-md-syntax" }),
        });
      }
      // Style intermediate lines
      const startLine = firstLine.number;
      const endLine = lastLine.number;
      for (let ln = startLine; ln <= endLine; ln++) {
        const line = doc.line(ln);
        decos.push({
          from: line.from,
          to: line.from,
          deco: Decoration.line({ class: "cm-md-frontmatter-raw" }),
        });
      }
    } else {
      decos.push({
        from: fmFrom,
        to: fmTo,
        deco: Decoration.replace({ widget: new FrontmatterWidget(yaml) }),
      });
    }
  }

  // Sort decorations by from position (required by RangeSetBuilder)
  decos.sort((a, b) => {
    if (a.from !== b.from) return a.from - b.from;
    const aIsLine = a.deco.spec && a.deco.spec.class && a.from === a.to;
    const bIsLine = b.deco.spec && b.deco.spec.class && b.from === b.to;
    if (aIsLine && !bIsLine) return -1;
    if (!aIsLine && bIsLine) return 1;
    return a.to - b.to;
  });

  for (const d of decos) {
    try {
      if (d.from <= d.to && d.from >= 0 && d.to <= state.doc.length) {
        builder.add(d.from, d.to, d.deco);
      }
    } catch (e) {
      // Skip decorations that cause RangeSet errors (overlaps, etc.)
    }
  }

  return builder.finish();
}

class HRWidget extends WidgetType {
  toDOM() {
    const hr = document.createElement("hr");
    hr.style.border = "none";
    hr.style.borderTop = "1px solid var(--primary-color-6)";
    hr.style.margin = "1em 0";
    return hr;
  }
  eq() { return true; }
}

// StateField-based decorations (can replace across line breaks)
const livePreviewField = StateField.define({
  create(state) {
    return buildDecorations(state);
  },
  update(decos, tr) {
    if (tr.docChanged || tr.selection) {
      return buildDecorations(tr.state);
    }
    // Rebuild when async syntax parsing completes (fixes formatting delay)
    const oldTree = syntaxTree(tr.startState);
    const newTree = syntaxTree(tr.state);
    if (oldTree !== newTree) {
      return buildDecorations(tr.state);
    }
    return decos;
  },
  provide(f) {
    return EditorView.decorations.from(f);
  },
});

export const livePreviewPlugin = livePreviewField;
