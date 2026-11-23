// Live Preview (Settings ▸ Editor, View ▸ Live Preview): the text shown as the preview has it, but where the cursor
// is. Everything of it is in this module, as one extension the editor holds in a Compartment: off, the editor is as
// it was without it. In a LaTeX file it is the figures under their lines alone.
//
// What is drawn is found by pure functions of the state, tested without an editor: `inlinePieces` for a range of the
// text (the classes of the lines and ranges, the section numbers, the marks to hide, the inline math to render), and
// `blocksOf` for the whole of it (display math). A ViewPlugin draws the pieces of the lines in view, again as the
// text, the view, the syntax tree or the selection change; a StateField draws the blocks, which change the height of
// the text (CodeMirror takes those from the state only).
//
// What is shown as typed, and what rendered, is said in one place, `revealer`: an element is shown as typed when the
// selection is on one of its lines (a selection over several lines shows all of them), or when a suggestion or comment
// of the review is on it (the review draws over the text). A mark is hidden by a replace decoration, never atomic: the
// cursor goes through the text as without Live Preview, and the line it comes to is shown as typed. A block revealed
// is its text, with its rendering kept under it.
//
// The renderings are the tooltips' (`HoverSources.math`: pandoc's MathML), each asked once while the editor lives
// (`Renderings`); an element is shown as typed until its rendering has come, and when it cannot be rendered.

import { syntaxTree } from "@codemirror/language";
import { StateEffect, StateField, type EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { inlineFigures } from "./figures.ts";
import { mathOf } from "./hover.ts";
import { outline } from "./outline.ts";
import { criticMarks } from "./review/criticMode.ts";
import { shownOf } from "./review/overlay.ts";
import type { HoverSources } from "./types.ts";

type SyntaxNode = ReturnType<ReturnType<typeof syntaxTree>["resolve"]>;

/** What Live Preview takes from the app: the images of the figures (a data URI; null: not found), the renderings of
 *  the tooltips (math, citations), and whether the file is LaTeX. */
export type LivePreviewSources = {
  image: (path: string) => Promise<string | null>;
  hover?: HoverSources;
  latex?: boolean;
};

/** A range of the text. */
export type Span = { from: number; to: number };

/** What an element is rendered as: a formula (by `HoverSources.math`). */
export type Rendering = { kind: "math"; tex: string; display: boolean };

/** What is drawn of the text: a class on a line (at its start), a class on a range, a section number before a
 *  heading, a range hidden (a mark) or rendered unless the element it is part of, `scope`, is revealed. */
export type Piece =
  | { type: "line"; at: number; cls: string }
  | { type: "mark"; from: number; to: number; cls: string }
  | { type: "number"; at: number; text: string }
  | { type: "hide"; from: number; to: number; scope: Span }
  | { type: "render"; from: number; to: number; scope: Span; what: Rendering };

/** A block of the text, whole lines from `from` to `to`, rendered in their place, or under them when revealed. */
export type Block = { type: "math"; from: number; to: number; what: Rendering };

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

const MATH = new Set(["InlineMath", "DisplayMath", "MathEnvironment"]);

/** The lines of a math node that is a block (display math alone on its lines, its attributes after it), or null. */
function mathBlockOf(state: EditorState, node: SyntaxNode): Span | null {
  if (node.name === "InlineMath") return null;
  let end = node.to;
  // `$$ x $$ {#eq:x}` on one line is a paragraph: its attributes are the next node
  const next = node.nextSibling;
  if (next?.name === "Attributes" && !state.sliceDoc(end, next.from).trim()) end = next.to;
  const first = state.doc.lineAt(node.from);
  const last = state.doc.lineAt(end);
  if (first.text.slice(0, node.from - first.from).trim() || state.sliceDoc(end, last.to).trim()) return null;
  return { from: first.from, to: last.to };
}

/** The blocks of the text, in order. */
export function blocksOf(state: EditorState): Block[] {
  const out: Block[] = [];
  syntaxTree(state).iterate({
    enter(node) {
      if (!MATH.has(node.name)) return;
      const lines = mathBlockOf(state, node.node);
      const math = lines && mathOf(state, node.node);
      if (lines && math?.tex) out.push({ type: "math", ...lines, what: { kind: "math", tex: math.tex, display: true } });
      return false;
    },
  });
  return out;
}

/** What is read from the whole text, again as it or its tree changes: the section numbers, by where their heading
 *  begins, and the blocks. */
export type Analysis = { numbers: Map<number, string>; blocks: Block[] };

/** The analysis of `state`: the headings of the tree as it is (not parsed further: this runs as the text is typed). */
export function analyse(state: EditorState): Analysis {
  const numbers = new Map<number, string>();
  for (const h of outline(state, false)) if (h.number) numbers.set(h.from, h.number);
  return { numbers, blocks: blocksOf(state) };
}

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

/** What is drawn of the text from `from` to `to` (the blocks apart). */
export function inlinePieces(state: EditorState, from: number, to: number, analysis: Analysis = analyse(state)): Piece[] {
  const out: Piece[] = [];
  const scoped = (scope: Span) => ({ from: scope.from, to: scope.to });
  const hide = (a: number, b: number, scope: Span) => a < b && out.push({ type: "hide", from: a, to: b, scope: scoped(scope) });
  syntaxTree(state).iterate({
    from,
    to,
    enter(node) {
      if (MARKS.has(node.name) && node.node.parent) {
        hide(node.from, node.to, node.node.parent);
        return;
      }
      if (MATH.has(node.name)) {
        // a block is drawn by the blocks; math over lines in a paragraph stays as typed
        if (mathBlockOf(state, node.node) || state.doc.lineAt(node.from).number !== state.doc.lineAt(node.to).number) return false;
        const math = mathOf(state, node.node);
        if (math?.tex) out.push({ type: "render", from: node.from, to: node.to, scope: scoped(node), what: { kind: "math", ...math } });
        return false;
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

/** What is rendered of a rendering, if it has come: HTML (math). */
export type Rendered = (what: Rendering) => string | null | undefined;

/** The blocks drawn in place of their lines where the selection is as it is: not revealed, their rendering come. */
export function blocksInPlace(state: EditorState, blocks: Block[], rendered: Rendered = () => ""): Block[] {
  const revealed = revealer(state);
  return blocks.filter((b) => !revealed(b.from, b.to) && typeof rendered(b.what) === "string");
}

/** What of the pieces is drawn where the selection is as it is: the ranges hidden (in order, those that touch merged)
 *  and those rendered (whose rendering has come, by `rendered`), none of the elements revealed, none across a line
 *  break, none in a block drawn in place (`covered`); a range hidden in one rendered is left to it. */
export function drawnPieces(state: EditorState, pieces: Piece[], rendered: Rendered = () => "", covered: Span[] = []) {
  const revealed = revealer(state);
  const oneLine = (s: Span) => state.doc.lineAt(s.from).number === state.doc.lineAt(s.to).number;
  const free = (s: Span) => !covered.some((c) => c.from < s.to && s.from < c.to);
  const shown: { from: number; to: number; html: string; what: Rendering }[] = [];
  for (const p of pieces) {
    if (p.type !== "render" || revealed(p.scope.from, p.scope.to) || !oneLine(p) || !free(p)) continue;
    const html = rendered(p.what);
    if (typeof html === "string") shown.push({ from: p.from, to: p.to, html, what: p.what });
  }
  const spans = pieces
    .flatMap((p) => (p.type === "hide" && !revealed(p.scope.from, p.scope.to) ? [{ from: p.from, to: p.to }] : []))
    .filter((s) => free(s) && !shown.some((r) => r.from < s.to && s.from < r.to))
    .sort((a, b) => a.from - b.from || a.to - b.to);
  const hidden: Span[] = [];
  for (const s of spans) {
    const last = hidden[hidden.length - 1];
    if (last && s.from <= last.to) last.to = Math.max(last.to, s.to);
    else hidden.push({ ...s });
  }
  return { hidden: hidden.filter(oneLine), rendered: shown };
}

/** The ranges hidden or rendered of the text from `from` to `to` where the selection is as it is, everything
 *  rendered as soon as asked (for the tests): the inline pieces, then the blocks. */
export function hiddenRanges(state: EditorState, from = 0, to = state.doc.length): Span[] {
  const analysis = analyse(state);
  const blocks = blocksInPlace(state, analysis.blocks).filter((b) => b.from <= to && b.to >= from);
  const { hidden, rendered } = drawnPieces(state, inlinePieces(state, from, to, analysis), undefined, blocks);
  return [...hidden, ...rendered, ...blocks].map((s) => ({ from: s.from, to: s.to })).sort((a, b) => a.from - b.from);
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

/** An element rendered: the HTML of its rendering, inline or as a block. A click on it puts the cursor in its text. */
class RenderedWidget extends WidgetType {
  readonly html: string;
  readonly block: boolean;
  constructor(html: string, block: boolean) {
    super();
    [this.html, this.block] = [html, block];
  }
  eq(other: RenderedWidget) {
    return other.html === this.html && other.block === this.block;
  }
  get estimatedHeight() {
    return this.block ? 48 : -1;
  }
  toDOM() {
    const dom = document.createElement(this.block ? "div" : "span");
    dom.className = this.block ? "cm-lp-block" : "cm-lp-inline";
    // the app's rendering (pandoc's MathML of the text typed: the text is escaped there), as the tooltips show it
    dom.innerHTML = this.html;
    return dom;
  }
  ignoreEvent() {
    return false;
  }
}

/** The renderings asked, by what they render: each asked once while the editor lives; the views told (`renderedEffect`)
 *  as they come, a moment after the first so that those that come together are drawn once. */
class Renderings {
  readonly results = new Map<string, string | null>();
  readonly asked = new Set<string>();
  readonly views = new Set<EditorView>();
  timer: ReturnType<typeof setTimeout> | undefined;
  readonly sources: HoverSources;
  constructor(sources: HoverSources) {
    this.sources = sources;
  }

  /** The rendering of `what` if it has come (null: it cannot be rendered); asked if it was not. */
  get = (what: Rendering): string | null | undefined => {
    const key = `${what.kind}:${what.display ? "display" : "inline"}:${what.tex}`;
    if (this.results.has(key)) return this.results.get(key);
    if (!this.asked.has(key)) {
      this.asked.add(key);
      const ask = this.sources.math ? this.sources.math(what.tex, what.display) : Promise.resolve(null);
      ask
        .then((html) => (html && html.includes("<math") ? html : null), () => null)
        .then((html) => {
          this.results.set(key, html);
          this.tell();
        });
    }
    return undefined;
  };

  tell() {
    if (this.timer) return;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      for (const view of this.views) view.dispatch({ effects: renderedEffect.of(null) });
    }, 30);
  }
}

/** Renderings have come. */
const renderedEffect = StateEffect.define<null>();

const hidden = Decoration.replace({});

/** The decorations of the text from `from` to `to`, where the selection is as it is. */
function inlineDecorations(state: EditorState, from: number, to: number, analysis: Analysis, rendered: Rendered): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  const pieces = inlinePieces(state, from, to, analysis);
  for (const p of pieces) {
    if (p.type === "line") ranges.push(Decoration.line({ class: p.cls }).range(p.at));
    else if (p.type === "mark") ranges.push(Decoration.mark({ class: p.cls }).range(p.from, p.to));
    else if (p.type === "number") ranges.push(Decoration.widget({ widget: new NumberWidget(p.text), side: -1 }).range(p.at));
  }
  const drawn = drawnPieces(state, pieces, rendered, blocksInPlace(state, analysis.blocks, rendered));
  for (const s of drawn.hidden) ranges.push(hidden.range(s.from, s.to));
  for (const r of drawn.rendered) ranges.push(Decoration.replace({ widget: new RenderedWidget(r.html, false) }).range(r.from, r.to));
  return Decoration.set(ranges, true);
}

/** The decorations of the blocks: rendered in place of their lines, or under them when revealed. */
function blockDecorations(state: EditorState, blocks: Block[], rendered: Rendered): DecorationSet {
  const inPlace = new Set(blocksInPlace(state, blocks, rendered));
  const ranges: Range<Decoration>[] = [];
  for (const b of blocks) {
    const html = rendered(b.what);
    if (typeof html !== "string") continue;
    const widget = new RenderedWidget(html, true);
    if (inPlace.has(b)) ranges.push(Decoration.replace({ widget, block: true }).range(b.from, b.to));
    else ranges.push(Decoration.widget({ widget, block: true, side: 1 }).range(b.to));
  }
  return Decoration.set(ranges, true);
}

/** Live Preview of a Markdown text, its renderings from `renderings`. */
function markdownPreview(renderings: Renderings): Extension {
  const rendered: Rendered = renderings.get;
  const field = StateField.define<{ analysis: Analysis; blocks: DecorationSet }>({
    create(state) {
      const analysis = analyse(state);
      return { analysis, blocks: blockDecorations(state, analysis.blocks, rendered) };
    },
    update(value, tr) {
      const parsed = syntaxTree(tr.state) !== syntaxTree(tr.startState);
      const analysis = tr.docChanged || parsed ? analyse(tr.state) : value.analysis;
      if (analysis === value.analysis && !tr.selection && !tr.effects.length) return value;
      return { analysis, blocks: blockDecorations(tr.state, analysis.blocks, rendered) };
    },
    provide: (f) => EditorView.decorations.from(f, (v) => v.blocks),
  });
  const inline = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      readonly view: EditorView;
      constructor(view: EditorView) {
        this.view = view;
        renderings.views.add(view);
        this.decorations = this.build();
      }
      update(u: ViewUpdate) {
        // an effect may be renderings come, or the review's items given (they reveal what they are on)
        const effects = u.transactions.some((t) => t.effects.length);
        if (u.docChanged || u.viewportChanged || u.selectionSet || effects || syntaxTree(u.state) !== syntaxTree(u.startState)) {
          this.decorations = this.build();
        }
      }
      build(): DecorationSet {
        const { state, viewport } = this.view;
        return inlineDecorations(state, viewport.from, viewport.to, state.field(field).analysis, rendered);
      }
      destroy() {
        renderings.views.delete(this.view);
      }
    },
    { decorations: (v) => v.decorations },
  );
  return [field, inline, theme];
}

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
  // math as the preview has it: display math centred on a line of its own
  ".cm-lp-block": { padding: "4px 0", textAlign: "center", overflowX: "auto", cursor: "text" },
  ".cm-lp-block math": { display: "block", margin: "0 auto" },
  ".cm-lp-inline": { cursor: "text" },
});

/** The extension of Live Preview. */
export function livePreview(sources: LivePreviewSources): Extension {
  const figures = inlineFigures(sources.image);
  if (sources.latex) return figures;
  return [markdownPreview(new Renderings(sources.hover ?? {})), figures];
}
