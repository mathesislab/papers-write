// The editor of the review and of the files of a project: CodeMirror 6. Markdown, the markup dimmed (the headings,
// the emphasis, the quotes drawn as they read), in the system font; or plain text in monospace. The font and size of
// the Markdown follow the settings, through --editor-font and --editor-size.

import {
  defaultKeymap,
  history,
  historyKeymap,
  indentLess,
  indentMore,
  isolateHistory,
  redoSelection,
  undoSelection,
} from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, indentUnit, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { Compartment, EditorState, StateEffect, StateField, Transaction, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  drawSelection,
  keymap,
  lineNumbers,
  placeholder,
  type DecorationSet,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { acceptCompletion, completion } from "./completion";
import { literalAt, type Literal } from "./context";
import { difference } from "./difference";
import { hover } from "./hover";
import * as lang from "./lang/index.ts";
import { suggesting } from "./review/mode.ts";
import { sections } from "./sections";
import { DEFAULTS, latex, typed, type Rule } from "./symbols";
import type { Completion, EditorFont, HoverSources, Indent } from "./types";

const style = HighlightStyle.define([
  { tag: tags.heading, fontWeight: "650", color: "var(--md-heading)" },
  // the marks: # * > ` and the brackets of links
  { tag: tags.processingInstruction, color: "var(--muted)", fontWeight: "400" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strong, fontWeight: "650" },
  { tag: tags.quote, color: "var(--md-quote)", fontStyle: "italic" },
  { tag: tags.link, color: "var(--accent)" },
  { tag: tags.url, color: "var(--muted)" },
  { tag: tags.monospace, fontFamily: "ui-monospace, Menlo, monospace", fontSize: "0.92em" },
  // front matter: the fences and the keys; math and LaTeX: the marks and braces dimmed, the commands coloured
  { tag: [lang.frontMatterMark, lang.mathMark, lang.mathBrace, lang.mathOperator, lang.latexBrace, lang.citeMark], color: "var(--muted)" },
  { tag: lang.frontMatterKey, color: "var(--md-meta, var(--md-heading))" },
  { tag: [lang.mathCommand, lang.latexCommand, lang.latexEnvName], color: "var(--md-math, var(--accent))" },
  { tag: lang.citationKey, color: "var(--md-cite, var(--accent))" },
  { tag: [lang.crossRefKey, lang.labelName], color: "var(--md-ref, var(--md-cite, var(--accent)))" },
  { tag: tags.comment, color: "var(--muted)", fontStyle: "italic" },
  { tag: lang.mathNumber, color: "var(--md-math-number, var(--md-quote, var(--muted)))" },
]);

/** The fonts of the settings. */
const FONTS: Record<EditorFont, string> = {
  system: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif",
  serif: '"New York", ui-serif, Georgia, serif',
  mono: "ui-monospace, Menlo, monospace",
};

/** The font and size of the settings set for the editors of this window (CSS variables of the page). */
export function setEditorFont(font: EditorFont, size: number) {
  const style = document.documentElement.style;
  style.setProperty("--editor-font", FONTS[font] ?? FONTS.system);
  style.setProperty("--editor-size", `${size}px`);
}

const theme = (mono: boolean) =>
  EditorView.theme({
    "&": { height: "100%", fontSize: mono ? "calc(var(--editor-size, 13.5px) - 1px)" : "var(--editor-size, 13.5px)", color: "var(--fg)", background: "transparent" },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": {
      overflow: "auto",
      fontFamily: mono ? "ui-monospace, Menlo, monospace" : `var(--editor-font, ${FONTS.system})`,
    },
    ".cm-content": { lineHeight: mono ? "1.5" : "1.6", padding: "10px 0 40px", caretColor: "var(--accent)" },
    ".cm-line": { padding: "0 18px" },
    ".cm-cursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "1.5px" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": { background: "var(--selection)" },
    ".cm-placeholder": { color: "var(--muted)" },
  });

/** The band at the top and foot of the editor where a selection being made with the mouse scrolls, in px. */
const EDGE = 24;

/** A selection made with the mouse scrolls the text at the top and foot of the editor, faster the further into the
 *  edge and the longer it is held there. CodeMirror's own scrolling goes only with the distance past the edge: the
 *  pane at the foot of the screen leaves little room below it, and the text crept down while it flew up. The
 *  selection follows each step, as a move of the mouse where it is. */
const dragScroll = ViewPlugin.fromClass(
  class {
    timer: ReturnType<typeof setInterval> | undefined;
    /** how long the mouse has been in the edge, in ms */
    held = 0;
    last: MouseEvent | null = null;
    constructor(readonly view: EditorView) {
      view.contentDOM.addEventListener("mousedown", this.down);
    }
    down = (e: MouseEvent) => {
      if (e.button !== 0) return;
      this.stop();
      [this.last, this.held] = [e, 0];
      const doc = this.view.contentDOM.ownerDocument;
      doc.addEventListener("mousemove", this.move);
      doc.addEventListener("mouseup", this.stop);
      this.timer = setInterval(this.step, 50);
    };
    move = (e: MouseEvent) => {
      if (e.isTrusted) this.last = e;
    };
    step = () => {
      const e = this.last;
      if (!e || e.buttons === 0) return this.stop();
      const { top, bottom } = this.view.scrollDOM.getBoundingClientRect();
      const into = e.clientY < top + EDGE ? e.clientY - top - EDGE : e.clientY > bottom - EDGE ? e.clientY - bottom + EDGE : 0;
      // a click held still is no selection
      if (!into || this.view.state.selection.main.empty) return void (this.held = 0);
      this.held += 50;
      this.view.scrollDOM.scrollTop += Math.sign(into) * (4 + 0.5 * Math.abs(into)) * Math.min(6, 1 + this.held / 300);
      const { clientX, clientY, buttons, shiftKey, altKey, metaKey } = e;
      e.view?.document.dispatchEvent(new MouseEvent("mousemove", { clientX, clientY, buttons, shiftKey, altKey, metaKey, bubbles: true }));
    };
    stop = () => {
      clearInterval(this.timer);
      this.timer = undefined;
      const doc = this.view.contentDOM.ownerDocument;
      doc.removeEventListener("mousemove", this.move);
      doc.removeEventListener("mouseup", this.stop);
    };
    destroy() {
      this.stop();
      this.view.contentDOM.removeEventListener("mousedown", this.down);
    }
  },
);

/** The section shown alone (its index), or the whole text (null). */
const showEffect = StateEffect.define<number | null>();

/** The rest of the text hidden around the section shown, as whole lines; the cursor cannot go into it. */
function hidden(text: string, index: number | null): DecorationSet {
  if (index === null) return Decoration.none;
  const all = sections(text);
  const section = all[Math.min(index, all.length - 1)];
  if (!section) return Decoration.none;
  const ranges = [];
  if (section.from > 0) ranges.push(Decoration.replace({ block: true }).range(0, section.from - 1));
  if (section.to < text.length) ranges.push(Decoration.replace({ block: true }).range(section.to, text.length));
  return Decoration.set(ranges);
}

const shown = StateField.define<{ index: number | null; hidden: DecorationSet }>({
  create: () => ({ index: null, hidden: Decoration.none }),
  update(value, tr) {
    let index = value.index;
    for (const e of tr.effects) if (e.is(showEffect)) index = e.value;
    if (index === value.index && !tr.docChanged) return value;
    return { index, hidden: hidden(tr.state.doc.toString(), index) };
  },
  provide: (field) => [
    EditorView.decorations.from(field, (value) => value.hidden),
    EditorView.atomicRanges.of((view) => view.state.field(field).hidden),
  ],
});

/** The section `index` alone, or the whole text (null); the cursor into the section if it was out of it. */
export function showSection(view: EditorView, index: number | null) {
  const text = view.state.doc.toString();
  const section = index === null ? undefined : sections(text)[index];
  const cursor = view.state.selection.main.head;
  const inside = section && cursor >= section.from && cursor <= section.to;
  view.dispatch({
    effects: showEffect.of(index),
    ...(section && !inside ? { selection: { anchor: Math.max(section.from, section.to - 1) }, scrollIntoView: true } : {}),
  });
  view.focus();
}

/** The sequences that become symbols as they are typed. */
const rulesEffect = StateEffect.define<readonly Rule[]>();
const symbolRules = StateField.define<readonly Rule[]>({
  create: () => DEFAULTS,
  update(rules, tr) {
    for (const e of tr.effects) if (e.is(rulesEffect)) return e.value;
    return rules;
  },
});

/** The sequences typed that become symbols set (see `merged` in symbols.ts), in an editor made as Markdown. */
export function setRules(view: EditorView, rules: readonly Rule[]) {
  view.dispatch({ effects: rulesEffect.of(rules) });
}

/** Whether `pos` is in code, where what is typed stays as typed: inside an inline code span or in a block of code. */
export function inCode(state: EditorState, pos: number): boolean {
  for (let node = syntaxTree(state).resolve(pos, -1); node.parent; node = node.parent) {
    if (node.name === "InlineCode") return node.from < pos && pos < node.to;
    if (node.name === "FencedCode" || node.name === "CodeBlock") return true;
  }
  return false;
}

/** What `pos` is in apart from code that keeps the symbols typed out: math, a LaTeX environment, the front matter, a
 *  key or a path (see `literalAt`). */
export function literalIn(state: EditorState, pos: number): Literal | null {
  const line = state.doc.lineAt(pos);
  return literalAt(syntaxTree(state), pos, line.text.slice(0, pos - line.from));
}

/** The `back` characters before the cursor replaced by `insert`, the cursor after it. The change is an undo step of its
 *  own: undone, what was typed is back. */
function replaceBack(view: EditorView, back: number, insert: string) {
  const head = view.state.selection.main.head;
  view.dispatch({
    changes: { from: head - back, to: head, insert },
    selection: { anchor: head - back + insert.length },
    annotations: isolateHistory.of("full"),
    userEvent: "input.symbol",
  });
}

/** A sequence typed that completes one of the rules: the character typed goes in, then the sequence is replaced. */
const typedSymbols = EditorView.inputHandler.of((view, from, to, text, insert) => {
  const { state } = view;
  if (view.composing || text.length !== 1 || from !== to || state.selection.ranges.length !== 1 || suggesting(state)) return false;
  const line = state.doc.lineAt(from);
  const hit = typed(line.text.slice(0, from - line.from), text, state.field(symbolRules));
  if (!hit || inCode(state, from) || literalIn(state, from)) return false;
  view.dispatch(insert());
  replaceBack(view, hit.from + 1, hit.insert);
  return true;
});

/** Tab after a LaTeX name (`\alpha`) gives its symbol; after anything else it is left to the indent. */
const latexTab = (view: EditorView) => {
  const { main } = view.state.selection;
  if (!main.empty || suggesting(view.state) || inCode(view.state, main.head) || literalIn(view.state, main.head)) return false;
  const line = view.state.doc.lineAt(main.head);
  const hit = latex(line.text.slice(0, main.head - line.from));
  if (hit) replaceBack(view, hit.length, hit.insert);
  return !!hit;
};

/** Tab: the indent of the settings at the cursor; with text selected, the lines of the selection indented. */
const tab = (view: EditorView) => {
  const { state } = view;
  if (state.selection.ranges.some((r) => !r.empty)) return indentMore(view);
  // in math and in LaTeX the text is left alone mid-line: no indent into a formula
  const { head } = state.selection.main;
  const kind = literalIn(state, head);
  if ((kind === "math" || kind === "latex") && state.doc.lineAt(head).text.slice(0, head - state.doc.lineAt(head).from).trim()) return true;
  view.dispatch(state.replaceSelection(state.facet(indentUnit)), { scrollIntoView: true, userEvent: "input.indent" });
  return true;
};

/** What the editors take from the settings: what Tab puts in, whether the text is a column of a readable width, and
 *  whether its lines are numbered. */
export type EditorPrefs = { indent: Indent; readable: boolean; lineNumbers: boolean };
export const PREFS: EditorPrefs = { indent: "two", readable: true, lineNumbers: false };

/** What `prefsFrom` reads: the editor's part of the app's settings (a structural type, the app's own fits it). */
export type EditorSettings = { indent: Indent; readable_width: boolean; line_numbers: boolean };

export const prefsFrom = (s: EditorSettings): EditorPrefs => ({
  indent: s.indent ?? PREFS.indent,
  readable: s.readable_width ?? PREFS.readable,
  lineNumbers: s.line_numbers ?? PREFS.lineNumbers,
});

/** The numbers of the lines, quiet, on the page's background. */
const numbered = [
  lineNumbers(),
  EditorView.theme({
    ".cm-gutters": { background: "transparent", border: "none", color: "var(--muted)" },
    ".cm-lineNumbers .cm-gutterElement": { padding: "0 4px 0 10px", fontSize: "0.85em", fontVariantNumeric: "tabular-nums" },
  }),
];

const INDENTS: Record<Indent, string> = { two: "  ", four: "    ", tab: "\t" };

/** The width of the column of text when it is readable, and the room the line numbers take beside it. */
const COLUMN = "46em";
const GUTTER = "3.5em";
/** The text as a column of `COLUMN` at most, in the middle, as Obsidian has it: the room left is a margin on each
 *  side (of the lines, so that a click there still goes into the text), down to the usual padding in a narrow window.
 *  With the line numbers, they stand beside the column. */
const readable = EditorView.theme({
  "&": { containerType: "inline-size" },
  // more specific than the theme's padding of the lines, which it overrides
  ".cm-content .cm-line": { paddingInline: `max(18px, calc((100cqw - ${COLUMN}) / 2))` },
  ".cm-gutters": { paddingLeft: `max(0px, calc((100cqw - ${COLUMN}) / 2 - ${GUTTER}))` },
  ".cm-gutters ~ .cm-content .cm-line": { paddingLeft: "18px" },
});

const prefsSlot = new Compartment();
const prefsOf = (prefs: EditorPrefs): Extension => [
  indentUnit.of(INDENTS[prefs.indent] ?? INDENTS.two),
  prefs.readable ? readable : [],
  prefs.lineNumbers ? numbered : [],
];

/** The settings of the editor changed. */
export function setPrefs(view: EditorView, prefs: EditorPrefs) {
  view.dispatch({ effects: prefsSlot.reconfigure(prefsOf(prefs)) });
}

/** A symbol put in at the cursor, in place of the selection. */
export function insertSymbol(view: EditorView, symbol: string) {
  view.dispatch(view.state.replaceSelection(symbol), { userEvent: "input.symbol", scrollIntoView: true });
  view.focus();
}

/** The history's keys but ⌘U and ⇧⌘U (undo and redo the selection): ⌘U is the menu's Quote Selection. */
const historyKeys = historyKeymap.filter((k) => k.run !== undoSelection && k.run !== redoSelection);

/** The kind of editor: `mono` is plain text in monospace, for a file that is not Markdown; `latex` is a LaTeX file,
 *  highlighted, in monospace, completed, and with no symbols typed (it stays ASCII); `rules` are the sequences
 *  that become symbols as they are typed (Markdown only); `prefs` those of the settings; `completion` what the app
 *  gives for the completion of citations, references and LaTeX (Markdown only; without it there is none); `extensions`
 *  are more. */
export type Options = {
  mono?: boolean;
  latex?: boolean;
  rules?: readonly Rule[];
  prefs?: EditorPrefs;
  completion?: Completion;
  /** what the app gives for the tooltips over formulas, citation keys and images (cross-references need none); none
   *  by default (Markdown and LaTeX only) */
  hover?: HoverSources;
  /** what an empty editor shows, the app's (Papers Review: how to write a review); none by default */
  placeholder?: string;
  extensions?: Extension[];
};

/** An editor of `text` in `parent`; `changed` with the text after each change typed. */
export function makeEditor(parent: HTMLElement, text: string, changed: (text: string) => void, options: Options = {}) {
  const latexFile = options.latex ?? false;
  const mono = (options.mono ?? false) || latexFile;
  const complete = (!mono || latexFile) && options.completion;
  return new EditorView({
    parent,
    state: EditorState.create({
      doc: text,
      extensions: [
        history(),
        drawSelection(),
        dragScroll,
        keymap.of([
          // an open list takes Tab before the symbols and the indent (Enter and Esc are the list's own keys)
          ...(complete ? [{ key: "Tab", run: acceptCompletion }] : []),
          ...(mono ? [] : [{ key: "Tab", run: latexTab }]),
          { key: "Tab", run: tab, shift: indentLess },
          ...defaultKeymap,
          ...historyKeys,
        ]),
        prefsSlot.of(prefsOf(options.prefs ?? PREFS)),
        ...(latexFile
          ? [lang.tex, syntaxHighlighting(style)]
          : mono
            ? []
            : [
              markdown({ extensions: lang.paperMarkdown }),
              symbolRules.init(() => options.rules ?? DEFAULTS),
              typedSymbols,
              syntaxHighlighting(style),
              shown,
            ]),
        ...(options.placeholder ? [placeholder(options.placeholder)] : []),
        ...(complete ? [completion(complete, latexFile)] : []),
        ...(options.hover && (!mono || latexFile) ? [hover(options.hover)] : []),
        theme(mono),
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (update.docChanged && update.transactions.some((t) => t.annotation(Transaction.userEvent))) {
            changed(update.state.doc.toString());
          }
        }),
        ...(options.extensions ?? []),
      ],
    }),
  });
}

/** The text replaced by `text` (read again from the file), not to be undone into: only the part that differs, so
 *  that the cursor and the history of what was typed elsewhere stay where they were. */
export function replaceText(view: EditorView, text: string) {
  const change = difference(view.state.doc.toString(), text);
  if (change.from === change.to && !change.insert) return;
  view.dispatch({ changes: change, annotations: Transaction.addToHistory.of(false) });
}

/** A quote put in at the cursor as a paragraph of its own, the cursor after it to write what it is about. */
export function insertQuote(view: EditorView, quote: string) {
  const { doc } = view.state;
  const { head } = view.state.selection.main;
  const line = doc.lineAt(head);
  const above = line.number > 1 ? doc.line(line.number - 1).text.trim() : "";
  // a blank line between it and the text before it, and the text after it
  const before = line.text.slice(0, head - line.from).trim() ? "\n\n" : line.text.trim() || !above ? "" : "\n";
  const after = line.text.slice(head - line.from).trim() ? "\n\n" : "";
  const cursor = head + before.length + quote.length + 1;
  view.dispatch({
    changes: { from: head, insert: `${before}${quote} ${after}` },
    selection: { anchor: cursor },
    scrollIntoView: true,
    userEvent: "input.quote",
  });
  view.focus();
}

/** The words of a text. */
export const wordsOf = (text: string) => text.split(/\s+/).filter((w) => /\w/.test(w)).length;

/** The words selected, in all the ranges of the selection. */
export const selectedWords = (state: EditorState) =>
  state.selection.ranges.reduce((n, r) => n + (r.empty ? 0 : wordsOf(state.sliceDoc(r.from, r.to))), 0);

/** "1,234 words", or "56 of 1,234 words" with words selected. */
export function wordCount(words: number, selected = 0): string {
  const all = `${words.toLocaleString()} word${words === 1 ? "" : "s"}`;
  return selected ? `${selected.toLocaleString()} of ${all}` : all;
}
