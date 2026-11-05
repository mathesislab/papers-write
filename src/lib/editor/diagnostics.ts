// Diagnostics: wavy underlines, with their reason on hover, under the keys the text cannot resolve. Cross-references to
// a label the document does not define, and labels it defines twice, are found from the syntax tree (`labelProblems`);
// citation keys the document's bibliography lacks are asked of the app (`Diagnostics.missingKeys`), 800 ms after
// the typing stops. Plain decorations, not @codemirror/lint. Opt-in: an editor without `diagnostics` draws none.

import { syntaxTree } from "@codemirror/language";
import { StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, hoverTooltip, type DecorationSet, type PluginValue, type Tooltip, type ViewUpdate } from "@codemirror/view";
import { isTex } from "./hover.ts";
import type { Diagnostics } from "./types.ts";

/** A key underlined: where, why, and how bad (an `error` is a reference that goes nowhere, a `warning` a citation not found). */
export type Problem = { from: number; to: number; message: string; severity: "error" | "warning"; kind: "label" | "duplicate" | "citation" };

/** The keys the syntax tree has under the node `name`, with where they are. */
function keysOf(state: EditorState, name: string): { key: string; from: number; to: number }[] {
  const found: { key: string; from: number; to: number }[] = [];
  syntaxTree(state).iterate({
    enter(n) {
      if (n.name === name) found.push({ key: state.sliceDoc(n.from, n.to), from: n.from, to: n.to });
    },
  });
  return found;
}

/** Whether a LaTeX file takes files in: its labels may be defined in them, so no reference can be said to be undefined. */
const takesFiles = (state: EditorState) => isTex(state) && /(^|[^\\])\\(?:input|include|subfile|import)\b/m.test(state.doc.toString());

/** The cross-references to a label the document does not define (`@fig:x`, `\ref{x}`), and the labels it defines more
 *  than once (`{#fig:x}`, `\label{x}`), each occurrence. In order of position. */
export function labelProblems(state: EditorState): Problem[] {
  const labels = keysOf(state, "LabelName");
  const count = new Map<string, number>();
  for (const l of labels) count.set(l.key, (count.get(l.key) ?? 0) + 1);
  const problems: Problem[] = [];
  for (const l of labels) {
    if ((count.get(l.key) ?? 0) > 1) {
      problems.push({ ...l, message: `label ${l.key} is defined more than once`, severity: "error", kind: "duplicate" });
    }
  }
  if (!takesFiles(state)) {
    for (const r of keysOf(state, "CrossRefKey")) {
      if (!count.has(r.key)) problems.push({ ...r, message: `no label ${r.key} in the document`, severity: "error", kind: "label" });
    }
  }
  return problems.sort((a, b) => a.from - b.from);
}

/** The citation keys of the text with where they are (a key cited twice is there twice). */
export const citationKeys = (state: EditorState) => keysOf(state, "CitationKey");

/** The keys the app said are missing: replaced with each answer. */
const missingEffect = StateEffect.define<ReadonlySet<string>>();

type Held = { missing: ReadonlySet<string>; problems: Problem[] };

const NONE: ReadonlySet<string> = new Set();

/** The problems of the text as it is: those of the labels, and the citations the app said are missing. */
function problemsOf(state: EditorState, missing: ReadonlySet<string>): Problem[] {
  const problems = labelProblems(state);
  if (missing.size) {
    for (const c of citationKeys(state)) {
      if (missing.has(c.key)) {
        problems.push({ ...c, message: `${c.key} is not in the bibliography; ⌘. / Add from library`, severity: "warning", kind: "citation" });
      }
    }
  }
  return problems.sort((a, b) => a.from - b.from);
}

const held = StateField.define<Held>({
  create: (state) => ({ missing: NONE, problems: problemsOf(state, NONE) }),
  update(value, tr) {
    let missing = value.missing;
    for (const e of tr.effects) if (e.is(missingEffect)) missing = e.value;
    if (missing === value.missing && !tr.docChanged) return value;
    return { missing, problems: problemsOf(tr.state, missing) };
  },
});

/** The problems drawn: a citation key being typed is left alone (it is not a key yet, and the completion is open). */
const drawn = (state: EditorState): Problem[] => {
  const head = state.selection.main.head;
  return state.field(held).problems.filter((p) => !(p.kind === "citation" && p.from <= head && head <= p.to));
};

const underline = (severity: Problem["severity"]) => Decoration.mark({ class: `cm-problem cm-problem-${severity}` });
const MARKS = { error: underline("error"), warning: underline("warning") };

const decorations = EditorView.decorations.compute([held, "selection"], (state): DecorationSet =>
  Decoration.set(drawn(state).map((p) => MARKS[p.severity].range(p.from, p.to)), true),
);

const theme = EditorView.theme({
  ".cm-problem": { textDecorationLine: "underline", textDecorationStyle: "wavy", textDecorationSkipInk: "none", textUnderlineOffset: "2px" },
  ".cm-problem-warning": { textDecorationColor: "var(--warning, #d98e04)" },
  ".cm-problem-error": { textDecorationColor: "var(--error, #c4321f)" },
  // the box is the editor's (editor.ts: every tooltip of the editor); this is what is in it
  ".cm-problem-tip": { padding: "4px 10px" },
});

/** The citation keys asked of the app, 800 ms after the last change (and at once for `recheck`). */
const DEBOUNCE = 800;

const asking = (app: Diagnostics) =>
  ViewPlugin.fromClass(
    class implements PluginValue {
      timer: ReturnType<typeof setTimeout> | undefined;
      /** the question being asked: an answer to an older one is dropped */
      latest = 0;
      gone = false;
      view: EditorView;
      constructor(view: EditorView) {
        this.view = view;
        this.later(0);
      }
      update(u: ViewUpdate) {
        if (u.transactions.some((t) => t.effects.some((e) => e.is(recheckEffect)))) this.later(0);
        else if (u.docChanged) this.later(DEBOUNCE);
      }
      later(ms: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => void this.ask(), ms);
      }
      async ask() {
        const ask = app.missingKeys;
        if (!ask) return;
        const keys = [...new Set(citationKeys(this.view.state).map((c) => c.key))];
        const mine = ++this.latest;
        const missing = keys.length ? await ask(keys).catch(() => null) : NONE;
        if (!missing || this.gone || mine !== this.latest) return;
        const old = this.view.state.field(held).missing;
        // nothing new: the state is left alone
        if (old.size === missing.size && [...missing].every((k) => old.has(k))) return;
        this.view.dispatch({ effects: missingEffect.of(missing) });
      }
      destroy() {
        this.gone = true;
        clearTimeout(this.timer);
      }
    },
  );

const recheckEffect = StateEffect.define<null>();

/** The citation keys asked of the app again now (the bibliography or the library changed). */
export function recheck(view: EditorView) {
  view.dispatch({ effects: recheckEffect.of(null) });
}

/** The reason of the problem under `pos`, on hover. */
const reasons = hoverTooltip(
  (view, pos): Tooltip | null => {
    const hit = drawn(view.state).find((p) => p.from <= pos && pos <= p.to);
    if (!hit) return null;
    const dom = document.createElement("div");
    dom.className = "cm-problem-tip";
    dom.textContent = hit.message;
    return { pos: hit.from, end: hit.to, above: true, create: () => ({ dom }) };
  },
  { hoverTime: 300 },
);

/** The diagnostics of an editor. */
export const diagnostics = (app: Diagnostics): Extension => [held, decorations, reasons, theme, asking(app)];
