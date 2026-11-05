// The completion of the editor (@codemirror/autocomplete), for an app that gives a `Completion`: citations (from the
// app), cross-references (the labels of the document), LaTeX names, and paths (from the app). Where it applies is
// decided in context.ts; in ordinary prose the popup never opens.

import { acceptCompletion, autocompletion, pickedCompletion, snippet, type Completion as Option, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { syntaxTree } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { contextAt, labelsOf, nameOptions, refOptions } from "./context.ts";
import { suggesting } from "./review/mode.ts";
import type { Completion } from "./types.ts";

/** Section names of the citations: those of the bibliography first. */
const IN_BIB = { name: "In the bibliography", rank: 0 };
const FROM_LIBRARY = { name: "From the library", rank: 1 };

/** The text from `from` to the cursor replaced by `insert`. */
const put = (insert: string, after?: () => void): Option["apply"] => (view, option, from, to) => {
  view.dispatch({
    changes: { from, to, insert },
    selection: { anchor: from + insert.length },
    userEvent: "input.complete",
    annotations: pickedCompletion.of(option),
  });
  after?.();
};

/** The source of the options: it answers by what the text before the cursor is in. */
export const completionSource = (app: Completion) => async (cx: CompletionContext): Promise<CompletionResult | null> => {
  const { state, pos } = cx;
  // in suggest mode what is typed is not in the text before the cursor
  if (suggesting(state)) return null;
  const line = state.doc.lineAt(pos);
  const context = contextAt(syntaxTree(state), pos, line.text.slice(0, pos - line.from));
  if (!context) return null;
  // the options are chosen here (by the beginning of the name), not by CodeMirror's fuzzy matching, and given again
  // after each key typed
  const result = (options: Option[]): CompletionResult | null => (options.length ? { from: context.from, options, filter: false } : null);
  switch (context.kind) {
    case "citation": {
      if (!app.citations) return null;
      const found = await app.citations(context.prefix);
      if (cx.aborted) return null;
      return result(
        found.map((o): Option => ({
          label: o.label,
          detail: o.inBib ? o.detail : `＋ ${o.detail ?? o.key}`,
          type: "text",
          section: o.inBib ? IN_BIB : FROM_LIBRARY,
          apply: put(o.key, () => app.oncite?.(o.key)),
        })),
      );
    }
    case "path": {
      if (!app.paths) return null;
      const found = await app.paths(context.prefix);
      if (cx.aborted) return null;
      return result(found.map((p): Option => ({ label: p, type: "file", apply: put(p) })));
    }
    case "ref": {
      const labels = labelsOf(syntaxTree(state), state.doc.toString());
      return result(refOptions(labels, context).map((o): Option => ({ label: o.label, type: "variable", apply: put(o.insert) })));
    }
    case "latex":
      return result(
        nameOptions(context).map(
          (n): Option => ({ label: n.name, detail: n.detail, type: "keyword", apply: n.snippet ? snippet(n.snippet) : put(n.name) }),
        ),
      );
  }
};

const popup = EditorView.theme({
  ".cm-tooltip.cm-tooltip-autocomplete": {
    background: "var(--bg)",
    color: "var(--fg)",
    border: "1px solid var(--border)",
    borderRadius: "8px",
    boxShadow: "0 6px 20px rgba(0,0,0,.18)",
    overflow: "hidden",
    fontFamily: "var(--editor-font, -apple-system, system-ui, sans-serif)",
  },
  ".cm-tooltip.cm-tooltip-autocomplete > ul": { fontFamily: "inherit", maxHeight: "16em" },
  ".cm-tooltip-autocomplete ul li": { padding: "2px 10px", color: "var(--fg)" },
  ".cm-tooltip-autocomplete ul li[aria-selected]": { background: "var(--selection)", color: "var(--fg)" },
  ".cm-completionDetail": { color: "var(--muted)", marginLeft: "0.8em", fontStyle: "normal" },
  ".cm-completionIcon": { display: "none" },
  ".cm-completionMatchedText": { textDecoration: "none", fontWeight: "650" },
  ".cm-completionSection": { color: "var(--muted)", fontSize: "0.85em", padding: "2px 10px" },
});

/** The completion given by the app. Enter accepts the option marked, and Esc closes the list, when it is open (the
 *  extension's own keys: Enter does what it did otherwise when it is not). Tab does so too, before the other keys of
 *  Tab; the keymap of the editor puts `acceptCompletion` first. */
export const completion = (app: Completion): Extension => [
  autocompletion({ override: [completionSource(app)], icons: false, defaultKeymap: true, closeOnBlur: true }),
  popup,
];

export { acceptCompletion };
