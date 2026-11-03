// The editor of the review and of the files of a project: CodeMirror 6. Markdown, the markup dimmed (the headings,
// the emphasis, the quotes drawn as they read, the page and line of a quote coloured), in the system font; or plain
// text in monospace. The font and size of the Markdown follow the settings, through --editor-font and --editor-size.

import { defaultKeymap, history, historyKeymap, indentWithTab, isolateHistory } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { EditorState, StateEffect, StateField, Transaction, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  drawSelection,
  keymap,
  placeholder,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { locationPrefix } from "./quote";
import { sections } from "./sections";
import { DEFAULTS, latex, typed, type Rule } from "./symbols";
import type { EditorFont } from "./types";

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
    ".cm-location": { color: "var(--md-location)", fontWeight: "550" },
  });

/** The page and line of a quote at the start of a line (`p5, l136–138:`) marked, in the lines in view. */
const locations = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = mark(view);
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) this.decorations = mark(update.view);
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

function mark(view: EditorView): DecorationSet {
  const ranges = [];
  let last = -1;
  for (const { from, to } of view.visibleRanges) {
    for (let pos = from; pos <= to; ) {
      const line = view.state.doc.lineAt(pos);
      pos = line.to + 1;
      if (line.from <= last) continue;
      last = line.from;
      const length = locationPrefix(line.text);
      if (length) ranges.push(Decoration.mark({ class: "cm-location" }).range(line.from, line.from + length));
    }
  }
  return Decoration.set(ranges);
}

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
  if (view.composing || text.length !== 1 || from !== to || state.selection.ranges.length !== 1) return false;
  const line = state.doc.lineAt(from);
  const hit = typed(line.text.slice(0, from - line.from), text, state.field(symbolRules));
  if (!hit || inCode(state, from)) return false;
  view.dispatch(insert());
  replaceBack(view, hit.from + 1, hit.insert);
  return true;
});

/** Tab after a LaTeX name (`\alpha`) gives its symbol; after anything else it is left to the indent. */
const latexTab = (view: EditorView) => {
  const { main } = view.state.selection;
  if (!main.empty) return false;
  const line = view.state.doc.lineAt(main.head);
  const hit = latex(line.text.slice(0, main.head - line.from));
  if (hit) replaceBack(view, hit.length, hit.insert);
  return !!hit;
};

/** A symbol put in at the cursor, in place of the selection. */
export function insertSymbol(view: EditorView, symbol: string) {
  view.dispatch(view.state.replaceSelection(symbol), { userEvent: "input.symbol", scrollIntoView: true });
  view.focus();
}

/** The kind of editor: `mono` is plain text in monospace, for a file that is not Markdown; `rules` are the sequences
 *  that become symbols as they are typed (Markdown only); `extensions` are more. */
export type Options = { mono?: boolean; rules?: readonly Rule[]; extensions?: Extension[] };

/** An editor of `text` in `parent`; `changed` with the text after each change typed. */
export function makeEditor(parent: HTMLElement, text: string, changed: (text: string) => void, options: Options = {}) {
  const mono = options.mono ?? false;
  return new EditorView({
    parent,
    state: EditorState.create({
      doc: text,
      extensions: [
        history(),
        drawSelection(),
        keymap.of([...(mono ? [] : [{ key: "Tab", run: latexTab }]), ...defaultKeymap, ...historyKeymap, indentWithTab]),
        ...(mono
          ? []
          : [
              markdown(),
              symbolRules.init(() => options.rules ?? DEFAULTS),
              typedSymbols,
              syntaxHighlighting(style),
              locations,
              placeholder("Your review, in Markdown: ## for its sections, a section a round, say."),
              shown,
            ]),
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

/** The whole text replaced (read again from the file), not to be undone into. */
export function replaceText(view: EditorView, text: string) {
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
    annotations: Transaction.addToHistory.of(false),
  });
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
