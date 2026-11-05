// Spelling: the system's spell checker (NSSpellChecker, in the app) on the prose of a Markdown editor, the underlines
// drawn by the editor itself. WebKit's own checking is off (`spellcheck="false"` on the content): it ignores that
// attribute on a span inside a checked line, so inline math and code were underlined. Here the editor decides what is prose
// (`proseRanges`: the visible lines but math, code, raw LaTeX, the front matter, citation and cross-reference keys,
// labels, URLs and link targets, attributes, HTML and comments), sends those texts to the app 500 ms after the typing
// stops (and on scroll), and draws a red dotted underline under each miss, mapped through the edits until the next
// check. Hover on a miss offers the guesses (a click replaces), Learn (the user's dictionary) and Ignore (this document).
// Opt-in: an editor without a `Spelling` draws none. Autocorrection and capitalisation stay off: the text is what was typed.

import { syntaxTree } from "@codemirror/language";
import { RangeSet, StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, hoverTooltip, type DecorationSet, type PluginValue, type Tooltip, type ViewUpdate } from "@codemirror/view";
import type { Spelling } from "./types.ts";

/** The nodes of the syntax tree that are not prose. */
const NOT_PROSE = new Set([
  "InlineMath",
  "DisplayMath",
  "MathEnvironment",
  "LatexEnvironment",
  "InlineCode",
  "FencedCode",
  "CodeBlock",
  "CitationKey",
  "CrossRefKey",
  "LabelName",
  "URL",
  "FrontMatter",
  "LatexCommand",
  "Attributes",
  "HTMLTag",
  "HTMLBlock",
  "Comment",
  "CommentBlock",
  "ProcessingInstruction",
  "ProcessingInstructionBlock",
]);

type Range = { from: number; to: number };

/** The ranges of `from` to `to` that are not prose, outermost only, in order. */
export function notProse(state: EditorState, from: number, to: number): Range[] {
  const ranges: Range[] = [];
  syntaxTree(state).iterate({
    from,
    to,
    enter(node) {
      if (!NOT_PROSE.has(node.name)) return;
      ranges.push({ from: node.from, to: node.to });
      return false;
    },
  });
  return ranges;
}

/** The ranges of `from` to `to` that are prose: what is left of it when `notProse` is taken out; none empty or only blank. */
export function proseRanges(state: EditorState, from: number, to: number): Range[] {
  const out: Range[] = [];
  let at = from;
  const add = (end: number) => {
    if (end > at && state.sliceDoc(at, end).trim()) out.push({ from: at, to: end });
  };
  for (const r of notProse(state, from, to)) {
    add(Math.min(r.from, to));
    at = Math.max(at, r.to);
  }
  add(to);
  return out;
}

/** The language the front matter asks for (`lang: en-GB`), if the text has a front matter and it does. */
export function languageOf(doc: string): string | undefined {
  const front = /^﻿?---[ \t]*\r?\n([^]*?)\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/.exec(doc);
  const lang = front && /^lang(?:uage)?:[ \t]*["']?([A-Za-z]{2,3}(?:[-_][A-Za-z0-9]+)*)["']?[ \t]*$/m.exec(front[1]);
  return lang?.[1];
}

/** The misses, replaced in the regions checked: `checked` are the ranges the answer covers. */
const missesEffect = StateEffect.define<{ checked: Range[]; misses: Range[] }>();
/** Ask the app again at once (a word was learned or ignored). */
const recheckEffect = StateEffect.define<null>();

const MISS = Decoration.mark({ class: "cm-misspelled" });

const held = StateField.define<RangeSet<typeof MISS>>({
  create: () => RangeSet.empty,
  update(set, tr) {
    set = set.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(missesEffect)) continue;
      const { checked, misses } = e.value;
      set = set.update({
        filter: (from, to) => !checked.some((c) => from >= c.from && to <= c.to),
        add: misses.map((m) => MISS.range(m.from, m.to)),
        sort: true,
      });
    }
    return set;
  },
});

/** The misses drawn: the word being typed (the cursor in it or at its end) is left alone until the cursor leaves it. */
function drawn(state: EditorState): Range[] {
  const sel = state.selection.main;
  const out: Range[] = [];
  state.field(held).between(0, state.doc.length, (from, to) => {
    if (sel.empty && sel.head >= from && sel.head <= to) return;
    out.push({ from, to });
  });
  return out;
}

const decorations = EditorView.decorations.compute([held, "selection"], (state): DecorationSet =>
  Decoration.set(drawn(state).map((m) => MISS.range(m.from, m.to))),
);

const theme = EditorView.theme({
  ".cm-misspelled": {
    textDecorationLine: "underline",
    textDecorationStyle: "dotted",
    textDecorationColor: "var(--spelling, #e5484d)",
    textDecorationThickness: "1.5px",
    textDecorationSkipInk: "none",
    textUnderlineOffset: "2px",
  },
  ".cm-spelling-tip": { padding: "4px 6px", display: "flex", flexWrap: "wrap", gap: "4px", maxWidth: "340px" },
  ".cm-spelling-tip button": {
    font: "inherit",
    padding: "1px 8px",
    border: "1px solid var(--border, #ccc)",
    borderRadius: "4px",
    background: "transparent",
    color: "inherit",
    cursor: "pointer",
  },
  ".cm-spelling-tip button:hover": { background: "var(--hover, rgba(128,128,128,.18))" },
  ".cm-spelling-tip .cm-spelling-none": { opacity: 0.6, padding: "1px 4px" },
  ".cm-spelling-tip .cm-spelling-sep": { flexBasis: "100%", height: 0 },
});

const DEBOUNCE = 500;
const SCROLL = 150;
const GUESSES = 6;

const checking = (app: Spelling) =>
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
        else if (u.docChanged || syntaxTree(u.state) !== syntaxTree(u.startState)) this.later(DEBOUNCE);
        else if (u.viewportChanged) this.later(SCROLL);
      }
      later(ms: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => void this.ask(), ms);
      }
      async ask() {
        const { state } = this.view;
        const checked = this.view.visibleRanges.map((r) => ({ from: r.from, to: r.to }));
        const pieces = checked.flatMap((r) => proseRanges(state, r.from, r.to));
        const mine = ++this.latest;
        const answers = pieces.length
          ? await app.check(pieces.map((p, id) => ({ id, text: state.sliceDoc(p.from, p.to) })), languageOf(state.doc.toString())).catch(() => null)
          : [];
        // the text changed meanwhile: its own check is on its way
        if (!answers || this.gone || mine !== this.latest || this.view.state.doc !== state.doc) return;
        const misses: Range[] = [];
        for (const a of answers) {
          const p = pieces[a.id];
          if (p) for (const m of a.misses) misses.push({ from: p.from + m.from, to: p.from + m.to });
        }
        this.view.dispatch({ effects: missesEffect.of({ checked, misses }) });
      }
      destroy() {
        this.gone = true;
        clearTimeout(this.timer);
      }
    },
  );

/** The guesses, Learn and Ignore over the miss under the pointer. */
const tip = (app: Spelling) =>
  hoverTooltip(
    async (view, pos): Promise<Tooltip | null> => {
      const hit = drawn(view.state).find((m) => m.from <= pos && pos <= m.to);
      if (!hit) return null;
      const doc = view.state.doc;
      const word = doc.sliceString(hit.from, hit.to);
      const guesses = await app.guesses(word, languageOf(doc.toString())).catch(() => [] as string[]);
      const dom = document.createElement("div");
      dom.className = "cm-spelling-tip";
      const button = (label: string, run: () => void) => {
        const b = document.createElement("button");
        b.type = "button";
        b.textContent = label;
        b.onmousedown = (e) => e.preventDefault();
        b.onclick = run;
        dom.append(b);
      };
      for (const g of guesses.slice(0, GUESSES)) {
        button(g, () => view.dispatch({ changes: { from: hit.from, to: hit.to, insert: g }, userEvent: "input.spelling" }));
      }
      if (!guesses.length) dom.append(Object.assign(document.createElement("span"), { className: "cm-spelling-none", textContent: "No suggestions" }));
      dom.append(Object.assign(document.createElement("span"), { className: "cm-spelling-sep" }));
      const again = () => view.dispatch({ effects: recheckEffect.of(null) });
      button("Learn", () => void Promise.resolve(app.learn(word)).then(again));
      button("Ignore", () => void Promise.resolve(app.ignore(word)).then(again));
      return { pos: hit.from, end: hit.to, above: true, create: () => ({ dom }) };
    },
    { hoverTime: 300 },
  );

/** The spelling of an editor: the app's checker on the prose, the misses underlined. */
export const spelling = (app: Spelling): Extension => [held, decorations, theme, checking(app), tip(app)];
