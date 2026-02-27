import { WidgetType } from "@codemirror/view";

/**
 * CM6 WidgetType that renders LaTeX via KaTeX.
 * Used as a Decoration.replace() widget when cursor is outside math nodes.
 */
export class KatexWidget extends WidgetType {
  constructor(latex, displayMode) {
    super();
    this.latex = latex;
    this.displayMode = displayMode;
  }

  eq(other) {
    return this.latex === other.latex && this.displayMode === other.displayMode;
  }

  toDOM() {
    const wrap = document.createElement(this.displayMode ? "div" : "span");
    wrap.className = this.displayMode
      ? "cm-katex-widget cm-katex-display"
      : "cm-katex-widget cm-katex-inline";

    if (typeof katex !== "undefined") {
      try {
        katex.render(this.latex, wrap, {
          displayMode: this.displayMode,
          throwOnError: false,
        });
      } catch (e) {
        wrap.textContent = this.latex;
      }
    } else {
      wrap.textContent = this.latex;
    }
    return wrap;
  }

  ignoreEvent() {
    return false;
  }
}
