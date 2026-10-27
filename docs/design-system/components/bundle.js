/* @ds-bundle: {"format":4,"namespace":"TerminalCompanion","components":[{"name":"Keycap"},{"name":"SuggestionRow"},{"name":"SuggestionPopup"},{"name":"InlineFallback"},{"name":"PermissionNotice"}]} */
(function () {
  var h = window.React.createElement;
  var GLYPH = { subcommand: "›", option: "-", path: "/", argument: "·" };

  function Keycap(props) {
    return h("kbd", { className: "tc-keycap footer" }, props.children);
  }

  // matches: [[start, end), ...] in code points of label, ascending, non-overlapping.
  function highlight(label, matches) {
    if (!matches || !matches.length) return label;
    var cps = Array.from(label), out = [], pos = 0;
    matches.forEach(function (m, i) {
      if (m[0] > pos) out.push(cps.slice(pos, m[0]).join(""));
      out.push(h("span", { key: i, className: "tc-match" }, cps.slice(m[0], m[1]).join("")));
      pos = m[1];
    });
    if (pos < cps.length) out.push(cps.slice(pos).join(""));
    return out;
  }

  function SuggestionRow(props) {
    var sel = !!props.selected;
    return h("div", {
      id: props.id, role: "option", "aria-selected": sel, title: props.label,
      className: "tc-row" + (sel ? " tc-row-selected" : "")
    },
      h("span", { className: "tc-glyph", "aria-hidden": "true" }, sel ? "❯" : GLYPH[props.kind || "subcommand"]),
      h("span", { className: "tc-label " + (sel ? "row-label-selected" : "row-label") }, highlight(props.label, props.matches)),
      props.description ? h("span", { className: "tc-desc row-description" }, props.description) : null
    );
  }

  // Visible window of `size` rows that always contains `sel`, moving by one row at a time.
  function windowStart(prevStart, sel, size, total) {
    var s = prevStart;
    if (sel < s) s = sel;
    if (sel >= s + size) s = sel - size + 1;
    return Math.max(0, Math.min(s, total - size));
  }

  function SuggestionPopup(props) {
    var items = props.items || [];
    if (!items.length) return null;
    var id = props.id || "tc-popup";
    var size = Math.min(props.maxRows || 8, items.length);
    var sel = Math.max(0, Math.min(props.selectedIndex || 0, items.length - 1));
    var start = windowStart(props.windowStart || 0, sel, size, items.length);
    var shown = items.slice(start, start + size);
    var cur = items[sel];
    return h("div", { className: "tc-popup", "data-window-start": start },
      h("div", { id: id, role: "listbox", "aria-label": "Suggestions", className: "tc-list" },
        shown.map(function (it, i) {
          return h(SuggestionRow, {
            key: start + i, id: id + "-opt-" + (start + i), label: it.label, description: it.description,
            kind: it.kind, matches: it.matches, selected: start + i === sel
          });
        })
      ),
      h("div", { className: "tc-footer footer" },
        h("span", { className: "tc-footer-ctx" }, props.context),
        items.length > size ? h("span", { className: "tc-count" }, (sel + 1) + "/" + items.length) : null,
        h("span", { className: "tc-footer-keys" },
          h(Keycap, null, "↑↓"), h(Keycap, null, "Tab"), h(Keycap, null, "Esc"))
      ),
      h("div", { className: "tc-sr", "aria-live": "polite", role: "status" },
        cur.label + (cur.description ? ", " + cur.description : "") + ", " + (sel + 1) + " of " + items.length)
    );
  }

  // before/after: the line text either side of the cursor; insertion: text the adapter will insert at the cursor.
  function InlineFallback(props) {
    return h("div", { className: "tc-inline row-label" },
      h("span", { className: "tc-prompt" }, props.prompt || "$"),
      h("span", { className: "tc-line" },
        " " + props.before,
        h("span", { className: "tc-ghost" }, props.insertion),
        props.after || ""),
      h("span", { className: "tc-inline-keys footer" },
        props.alternatives ? h("span", { className: "tc-ghost" }, "+" + props.alternatives + " more") : null,
        h(Keycap, null, "Tab"))
    );
  }

  function PermissionNotice(props) {
    return h("div", { className: "tc-notice row-description", role: "note" },
      h("span", null, props.denied ? "Not available: " : "Needs ", h("strong", { className: "row-label" }, props.capability), " — " + props.reason),
      props.recovery ? h("span", { className: "tc-recovery" }, " " + props.recovery) : null
    );
  }

  window.TerminalCompanion = {
    Keycap: Keycap, SuggestionRow: SuggestionRow, SuggestionPopup: SuggestionPopup,
    InlineFallback: InlineFallback, PermissionNotice: PermissionNotice
  };
})();
