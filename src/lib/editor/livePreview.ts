// Live Preview (Settings ▸ Editor, View ▸ Live Preview): the text shown as the preview has it, but where the cursor
// is. Everything of it is in this module, as one extension the editor holds in a Compartment: off, the editor is as
// it was without it. In a LaTeX file it is the figures under their lines alone.
//
// What is drawn is found by pure functions of the state, tested without an editor: `inlinePieces` for a range of the
// text (the classes of the lines and ranges, the section numbers, the marks to hide). A ViewPlugin draws those of the
// lines in view, again as the text, the view, the syntax tree or the selection change.
//
// What is shown as typed, and what rendered, is said in one place, `revealer`: an element is shown as typed when the
// selection is on one of its lines (a selection over several lines shows all of them), or when a suggestion or comment
// of the review is on it (the review draws over the text). A mark is hidden by a replace decoration, never atomic: the
// cursor goes through the text as without Live Preview, and the line it comes to is shown as typed.

import { syntaxTree } from "@codemirror/language";
import { StateField, type EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { inlineFigures } from "./figures.ts";
import { outline } from "./outline.ts";
import { criticMarks } from "./review/criticMode.ts";
import { shownOf } from "./review/overlay.ts";
import type { HoverSources } from "./types.ts";

/** What Live Preview takes from the app: the images of the figures (a data URI; null: not found), the renderings of
 *  the tooltips (math, citations), and whether the file is LaTeX. */
export type LivePreviewSources = {
  image: (path: string) => Promise<string | null>;
  hover?: HoverSources;
  latex?: boolean;
};

/** A range of the text. */
export type Span = { from: number; to: number };

/** What is drawn of the text: a class on a line (at its start), a class on a range, a section number before a
 *  heading, a range hidden (a mark) unless the element it is part of, `scope`, is revealed. */
export type Piece =
  | { type: "line"; at: number; cls: string }
  | { type: "mark"; from: number; to: number; cls: string }
  | { type: "number"; at: number; text: string }
  | { type: "hide"; from: number; to: number; scope: Span };

/** Whether the element from `from` to `to` is shown as typed: the selection is on one of its lines, or an item of the
 *  review is on it. Made once for a state, asked of each element. */
export function revealer(state: EditorState): (from: number, to: number) => boolean {
  const lines = state.selection.ranges.map((r) => ({ from: state.doc.lineAt(r.from).from, to: state.doc.lineAt(r.to).to }));
  const review = shownOf(state);
  return (from, to) => {
    const start = state.doc.lineAt(from).from;
    const end = state.doc.lineAt(to).to;
    return lines.some((l) => l.from <= end && l.to >= start) || review.some((s) => s.from <= to && s.to >= from);
  };
}

/** What is read from the whole text, again as it or its tree changes: the section numbers, by where their heading
 *  begins. */
export type Analysis = { numbers: Map<number, string> };

/** The analysis of `state`: the headings of the tree as it is (not parsed further: this runs as the text is typed). */
export function analyse(state: EditorState): Analysis {
  const numbers = new Map<number, string>();
  for (const h of outline(state, false)) if (h.number) numbers.set(h.from, h.number);
  return { numbers };
}

const analysisField = StateField.define<Analysis>({
  create: analyse,
  update: (value, tr) => (tr.docChanged || syntaxTree(tr.state) !== syntaxTree(tr.startState) ? analyse(tr.state) : value),
});

const HEADING = /^(?:ATX|Setext)Heading([1-6])$/;
/** The marks hidden with the element they are in (their parent). */
const MARKS = new Set(["EmphasisMark", "CodeMark", "CodeInfo"]);

/** How many spaces and tabs there are in `text` from `at` on (`dir` 1) or before it (-1). */
function blanks(text: string, at: number, dir: 1 | -1): number {
  let n = 0;
  while (/[ \t]/.test(text[dir > 0 ? at + n : at - n - 1] ?? "")) n++;
  return n;
}

/** A class on each line from `from` to `to` that is in [`start`, `end`]. */
function linesOf(state: EditorState, from: number, to: number, start: number, end: number, cls: string, out: Piece[]) {
  const last = state.doc.lineAt(Math.min(to, end)).number;
  for (let n = state.doc.lineAt(Math.max(from, start)).number; n <= last; n++) out.push({ type: "line", at: state.doc.line(n).from, cls });
}

/** The delimiters of the CriticMarkup marks from `from` to `to` (`{++`, `~>`, `<<}` ...), hidden unless their mark is
 *  revealed. */
function criticPieces(state: EditorState, from: number, to: number, out: Piece[]) {
  for (const m of criticMarks(state)) {
    if (m.to < from || m.from > to) continue;
    const scope = { from: m.from, to: m.to };
    const hide = (a: number, b: number) => a < b && out.push({ type: "hide", from: a, to: b, scope });
    const main = m.new ?? m.old;
    if (main) {
      hide(m.from, (m.old ?? main).from);
      if (m.old && m.new) hide(m.old.to, m.new.from);
      hide(main.to, main.to + 3);
    }
    // the note: after the mark, or the whole of a comment on its own
    const note = main ? main.to + 3 : m.from;
    if (note < m.to) {
      hide(note, note + 3);
      hide(m.to - 3, m.to);
    }
  }
}

/** What is drawn of the text from `from` to `to`. */
export function inlinePieces(state: EditorState, from: number, to: number, analysis: Analysis = analyse(state)): Piece[] {
  const out: Piece[] = [];
  const hide = (a: number, b: number, scope: Span) => a < b && out.push({ type: "hide", from: a, to: b, scope: { from: scope.from, to: scope.to } });
  syntaxTree(state).iterate({
    from,
    to,
    enter(node) {
      if (MARKS.has(node.name) && node.node.parent) {
        hide(node.from, node.to, node.node.parent);
        return;
      }
      const heading = HEADING.exec(node.name);
      if (heading) {
        // the line of the text: a Setext heading's underline is not
        const line = state.doc.lineAt(node.from);
        if (line.from >= from) {
          out.push({ type: "line", at: line.from, cls: `cm-lp-h${heading[1]}` });
          const number = analysis.numbers.get(node.from);
          if (number) out.push({ type: "number", at: line.from, text: number });
        }
        return;
      }
      switch (node.name) {
        case "HeaderMark": {
          // `# ` with its space, a closing ` ##` with the spaces before it, a Setext underline
          const line = state.doc.lineAt(node.from);
          const parent = node.node.parent ?? node;
          if (!line.text.slice(0, node.from - line.from).trim()) hide(node.from, node.to + blanks(line.text, node.to - line.from, 1), parent);
          else hide(node.from - blanks(line.text, node.from - line.from, -1), node.to, parent);
          return;
        }
        case "QuoteMark": {
          const line = state.doc.lineAt(node.from);
          hide(node.from, node.to + Math.min(1, blanks(line.text, node.to - line.from, 1)), node);
          return;
        }
        case "Attributes": {
          const line = state.doc.lineAt(node.from);
          hide(node.from - blanks(line.text, node.from - line.from, -1), node.to, node);
          return false;
        }
        case "Escape":
          hide(node.from, node.from + 1, node);
          return false;
        case "Link": {
          // `[text](url "title")` as its text, a link; `[text][label]` and `[text]` too
          const marks = node.node.getChildren("LinkMark");
          if (marks.length < 2) return;
          hide(marks[0].from, marks[0].to, node);
          hide(marks[1].from, node.to, node);
          out.push({ type: "mark", from: marks[0].to, to: marks[1].from, cls: "cm-lp-link" });
          return;
        }
        case "Autolink": {
          const marks = node.node.getChildren("LinkMark");
          if (marks.length < 2) return false;
          hide(marks[0].from, marks[0].to, node);
          hide(marks[1].from, marks[1].to, node);
          out.push({ type: "mark", from: marks[0].to, to: marks[1].from, cls: "cm-lp-link" });
          return false;
        }
        case "Image":
          return false;
        case "Blockquote":
          linesOf(state, node.from, node.to, from, to, "cm-lp-quote", out);
          return;
        case "FencedCode":
          // its fences hidden with the whole block (MARKS)
          linesOf(state, node.from, node.to, from, to, "cm-lp-codeblock", out);
          return;
        case "CodeBlock":
          linesOf(state, node.from, node.to, from, to, "cm-lp-codeblock", out);
          return false;
        case "InlineCode":
          out.push({ type: "mark", from: node.from, to: node.to, cls: "cm-lp-code" });
          return;
      }
    },
  });
  criticPieces(state, from, to, out);
  return out;
}

/** The ranges hidden of the text from `from` to `to` where the selection is as it is, in order, those that touch
 *  merged; none crosses a line break. */
export function hiddenRanges(state: EditorState, from = 0, to = state.doc.length, pieces = inlinePieces(state, from, to)): Span[] {
  const revealed = revealer(state);
  const spans = pieces
    .flatMap((p) => (p.type === "hide" && !revealed(p.scope.from, p.scope.to) ? [{ from: p.from, to: p.to }] : []))
    .sort((a, b) => a.from - b.from || a.to - b.to);
  const out: Span[] = [];
  for (const s of spans) {
    const last = out[out.length - 1];
    if (last && s.from <= last.to) last.to = Math.max(last.to, s.to);
    else out.push({ ...s });
  }
  // a replace decoration of a plugin stays on its line
  return out.filter((s) => state.doc.lineAt(s.from).number === state.doc.lineAt(s.to).number);
}

/** A section number before its heading. */
class NumberWidget extends WidgetType {
  readonly text: string;
  constructor(text: string) {
    super();
    this.text = text;
  }
  eq(other: NumberWidget) {
    return other.text === this.text;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-lp-number";
    span.textContent = this.text;
    return span;
  }
  ignoreEvent() {
    return false;
  }
}

const hidden = Decoration.replace({});

/** The decorations of the text from `from` to `to`, where the selection is as it is. */
function decorationsOf(state: EditorState, from: number, to: number, analysis: Analysis): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  const pieces = inlinePieces(state, from, to, analysis);
  for (const p of pieces) {
    if (p.type === "line") ranges.push(Decoration.line({ class: p.cls }).range(p.at));
    else if (p.type === "mark") ranges.push(Decoration.mark({ class: p.cls }).range(p.from, p.to));
    else if (p.type === "number") ranges.push(Decoration.widget({ widget: new NumberWidget(p.text), side: -1 }).range(p.at));
  }
  for (const s of hiddenRanges(state, from, to, pieces)) ranges.push(hidden.range(s.from, s.to));
  return Decoration.set(ranges, true);
}

/** The pieces of the lines in view, drawn. */
const inline = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }
    update(u: ViewUpdate) {
      // an effect may be the review's items given, which reveal what they are on
      const effects = u.transactions.some((t) => t.effects.length);
      if (u.docChanged || u.viewportChanged || u.selectionSet || effects || syntaxTree(u.state) !== syntaxTree(u.startState)) {
        this.decorations = this.build(u.view);
      }
    }
    build(view: EditorView): DecorationSet {
      const { from, to } = view.viewport;
      return decorationsOf(view.state, from, to, view.state.field(analysisField));
    }
  },
  { decorations: (v) => v.decorations },
);

const theme = EditorView.baseTheme({
  // the sizes of the preview's headings (Preview.svelte)
  ".cm-lp-h1": { fontSize: "1.6em", lineHeight: "1.3" },
  ".cm-lp-h2": { fontSize: "1.3em", lineHeight: "1.3" },
  ".cm-lp-h3": { fontSize: "1.1em" },
  ".cm-lp-h4": { fontStyle: "italic" },
  ".cm-lp-number": { marginRight: "0.5em" },
  // a bar at the left of a quote, and the code on a shade: drawn from the start of the text of the line (the line's
  // own padding is the column's margin)
  ".cm-lp-quote": {
    backgroundImage: "linear-gradient(var(--muted), var(--muted))",
    backgroundRepeat: "no-repeat",
    backgroundOrigin: "content-box",
    backgroundSize: "3px 100%",
    backgroundPosition: "-12px 0",
  },
  ".cm-lp-codeblock": {
    backgroundImage: "linear-gradient(rgba(127, 127, 127, 0.12), rgba(127, 127, 127, 0.12))",
    backgroundRepeat: "no-repeat",
    backgroundOrigin: "content-box",
    backgroundSize: "calc(100% + 16px) 100%",
    backgroundPosition: "-8px 0",
  },
  ".cm-lp-code": { background: "rgba(127, 127, 127, 0.14)", borderRadius: "3px" },
  ".cm-lp-link": { color: "var(--accent)" },
});

/** The extension of Live Preview. */
export function livePreview(sources: LivePreviewSources): Extension {
  const figures = inlineFigures(sources.image);
  if (sources.latex) return figures;
  return [analysisField, inline, figures, theme];
}
