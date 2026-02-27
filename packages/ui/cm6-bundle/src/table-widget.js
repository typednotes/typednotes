import { WidgetType } from "@codemirror/view";

/**
 * CM6 WidgetType that renders a pipe-separated markdown table as an HTML <table>.
 * Used as a Decoration.replace() widget when cursor is outside Table nodes.
 */
export class TableWidget extends WidgetType {
  constructor(text) {
    super();
    this.text = text;
  }

  eq(other) {
    return this.text === other.text;
  }

  toDOM() {
    const lines = this.text.split("\n").filter((l) => l.trim());
    if (lines.length < 2) {
      const span = document.createElement("span");
      span.textContent = this.text;
      return span;
    }

    const parseRow = (line) =>
      line
        .replace(/^\|/, "")
        .replace(/\|$/, "")
        .split("|")
        .map((cell) => cell.trim());

    const headerCells = parseRow(lines[0]);

    // Detect alignment from separator row (line 1)
    const sepCells = parseRow(lines[1]);
    const alignments = sepCells.map((sep) => {
      const s = sep.trim();
      if (s.startsWith(":") && s.endsWith(":")) return "center";
      if (s.endsWith(":")) return "right";
      return "left";
    });

    const bodyRows = lines.slice(2).map(parseRow);

    const table = document.createElement("table");
    table.className = "cm-md-table";

    // thead
    const thead = document.createElement("thead");
    const headRow = document.createElement("tr");
    headerCells.forEach((cell, i) => {
      const th = document.createElement("th");
      th.textContent = cell;
      if (alignments[i]) th.style.textAlign = alignments[i];
      headRow.appendChild(th);
    });
    thead.appendChild(headRow);
    table.appendChild(thead);

    // tbody
    if (bodyRows.length > 0) {
      const tbody = document.createElement("tbody");
      bodyRows.forEach((row) => {
        const tr = document.createElement("tr");
        headerCells.forEach((_, i) => {
          const td = document.createElement("td");
          td.textContent = row[i] || "";
          if (alignments[i]) td.style.textAlign = alignments[i];
          tr.appendChild(td);
        });
        tbody.appendChild(tr);
      });
      table.appendChild(tbody);
    }

    const wrap = document.createElement("div");
    wrap.className = "cm-md-table-wrap";
    wrap.appendChild(table);
    return wrap;
  }

  ignoreEvent() {
    return false;
  }
}
