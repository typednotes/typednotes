import { WidgetType } from "@codemirror/view";

/**
 * CM6 WidgetType that renders YAML frontmatter as a collapsed badge.
 * Used as a Decoration.replace() widget when cursor is outside the frontmatter block.
 */
export class FrontmatterWidget extends WidgetType {
  constructor(yaml) {
    super();
    this.yaml = yaml;
  }

  eq(other) {
    return this.yaml === other.yaml;
  }

  toDOM() {
    const badge = document.createElement("div");
    badge.className = "cm-frontmatter-badge";

    const chip = document.createElement("span");
    chip.className = "cm-frontmatter-chip";
    chip.textContent = "frontmatter";
    badge.appendChild(chip);

    return badge;
  }

  ignoreEvent() {
    return false;
  }
}
