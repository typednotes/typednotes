import { WidgetType } from "@codemirror/view";

/**
 * CM6 WidgetType that renders syntax-highlighted code via highlight.js.
 * Used as a Decoration.replace() widget when cursor is outside code nodes.
 */
export class HighlightWidget extends WidgetType {
  constructor(code, language, isBlock) {
    super();
    this.code = code;
    this.language = language;
    this.isBlock = isBlock;
  }

  eq(other) {
    return (
      this.code === other.code &&
      this.language === other.language &&
      this.isBlock === other.isBlock
    );
  }

  toDOM() {
    const codeEl = document.createElement("code");

    if (typeof hljs !== "undefined") {
      try {
        let result;
        if (this.language) {
          result = hljs.highlight(this.code, { language: this.language });
        } else {
          result = hljs.highlightAuto(this.code);
        }
        codeEl.innerHTML = result.value;
      } catch (e) {
        codeEl.textContent = this.code;
      }
    } else {
      codeEl.textContent = this.code;
    }

    if (this.isBlock) {
      const pre = document.createElement("pre");
      pre.className = "cm-hljs-widget cm-hljs-block";
      pre.appendChild(codeEl);
      return pre;
    } else {
      codeEl.className = "cm-hljs-widget cm-hljs-inline";
      return codeEl;
    }
  }

  ignoreEvent() {
    return false;
  }
}
