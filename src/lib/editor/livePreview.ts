// Live Preview (Settings ▸ Editor, View ▸ Live Preview): the text shown as the preview has it, but where the cursor
// is. Everything of it is in this module, as one extension the editor holds in a Compartment: off, the editor is as
// it was without it. In a LaTeX file it is the figures under their lines alone.
//
// What is drawn is found by pure functions of the state, tested without an editor: `inlinePieces` for a range of the
// text (the classes of the lines and ranges, the section numbers, the marks to hide, the inline math to render), and
// `blocksOf` for the whole of it (display math, figures, the front matter). A ViewPlugin draws the pieces of the lines
// in view, again as the text, the view, the syntax tree or the selection change; a StateField draws the blocks, which
// change the height of the text (CodeMirror takes those from the state only).
//
// What is shown as typed, and what rendered, is said in one place, `revealer`: an element is shown as typed when the
// selection is on one of its lines (a selection over several lines shows all of them), or when a suggestion or comment
// of the review is on it (the review draws over the text). A mark is hidden by a replace decoration, never atomic: the
// cursor goes through the text as without Live Preview, and the line it comes to is shown as typed. A block revealed
// is its text, with its rendering kept under it; a figure that shares its line with other text is always so.
//
// The renderings are the tooltips' (`HoverSources.math`: pandoc's MathML; `citation`: a reference's label, "Schoof
// (2007)") and the figures' images, each asked once while the editor lives (`Renderings`); an element is shown as typed
// until its rendering has come, and when it cannot be rendered (an image or a citation key not found: the key's
// underline stays in sight). Cross-references read as the label's number ("Figure 2", `labelNumbers`), which the
// editor finds itself; one to a label it does not know stays as typed.

import { syntaxTree } from "@codemirror/language";
import { StateEffect, StateField, type EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { INCLUDE, imagePath, isFile, labelNumbers, mathOf, refText } from "./hover.ts";
import { refKindOf } from "./lang/cite.ts";
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

/** What an element is rendered as: a formula (by `HoverSources.math`), a figure (its image by `image`), the front
 *  matter (its title). */
export type Rendering =
  | { kind: "math"; tex: string; display: boolean; number?: string }
  | { kind: "image"; path: string; caption?: string }
  | { kind: "front"; title?: string }
  | { kind: "cite"; parts: CitePart[]; group: boolean };

/** A part of a citation: its text as typed (a prefix, a locator, `; `), a citation key (`suppress`: `-@key`, the year
 *  alone), or a cross-reference as it reads ("Figure 2"). */
export type CitePart = string | { key: string; suppress: boolean } | { ref: string };

/** The narrative form of a reference is its label ("Schoof (2007)"); in brackets, its parenthetical form ("Schoof
 *  2007"), or the year alone (`-@key`). */
function citeForm(label: string, group: boolean, suppress: boolean): string {
  const m = /^(.*?)\s*\(([^()]*)\)$/.exec(label);
  if (!m || !group) return label;
  return suppress ? m[2] : `${m[1]} ${m[2]}`;
}

/** What a citation reads, its references' labels from `label` (undefined: not come yet; null: not found): "Schoof
 *  (2007)", "(see Schoof 2007, p. 3; Smith 2010)", "Figure 2". Undefined until every label has come, null if one is
 *  not found. */
export function citationText(what: { parts: CitePart[]; group: boolean }, label: (key: string) => string | null | undefined): string | null | undefined {
  let text = "";
  let cites = false;
  // every label asked, before any answer is waited for
  let missing: null | undefined | false = false;
  for (const part of what.parts) {
    if (typeof part === "string") text += part;
    else if ("ref" in part) text += part.ref;
    else {
      const found = label(part.key);
      if (found == null) missing = missing === null ? null : found;
      else text += citeForm(found, what.group, part.suppress);
      cites = true;
    }
  }
  if (missing !== false) return missing;
  // references alone in brackets read without them, as the preview has them
  return what.group && cites ? `(${text.trim()})` : text.trim();
}

/** What is drawn of the text: a class on a line (at its start), a class on a range, a section number before a
 *  heading, a range hidden (a mark) or rendered unless the element it is part of, `scope`, is revealed. */
export type Piece =
  | { type: "line"; at: number; cls: string }
  | { type: "mark"; from: number; to: number; cls: string }
  | { type: "number"; at: number; text: string }
  | { type: "hide"; from: number; to: number; scope: Span }
  | { type: "render"; from: number; to: number; scope: Span; what: Rendering };

/** A block of the text, whole lines from `from` to `to`, rendered in their place, or under them when revealed or not
 *  `alone` on them. */
export type Block = { from: number; to: number; alone: boolean; what: Rendering };

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

/** The lines of `node` and of its attributes after it; `alone`: nothing else is on them. */
function linesOfNode(state: EditorState, node: SyntaxNode): Span & { alone: boolean } {
  let end = node.to;
  // `$$ x $$ {#eq:x}` and `![a](b) {#fig:x}` are a paragraph: the attributes are the next node
  const next = node.nextSibling;
  if (next?.name === "Attributes" && !state.sliceDoc(end, next.from).trim()) end = next.to;
  const first = state.doc.lineAt(node.from);
  const last = state.doc.lineAt(end);
  const alone = !first.text.slice(0, node.from - first.from).trim() && !state.sliceDoc(end, last.to).trim();
  return { from: first.from, to: last.to, alone };
}

/** Whether a math node is a block: display math alone on its lines (its attributes after it). */
const isMathBlock = (state: EditorState, node: SyntaxNode) => node.name !== "InlineMath" && linesOfNode(state, node).alone;

/** The figure label of the attributes after `node` (`![a](b){#fig:x}`), if any. */
function figureLabel(state: EditorState, node: SyntaxNode): string | undefined {
  const next = node.nextSibling;
  if (next?.name !== "Attributes" || state.sliceDoc(node.to, next.from).trim()) return undefined;
  const label = next.getChildren("LabelName").map((n) => state.sliceDoc(n.from, n.to));
  return label.find((l) => refKindOf(l) === "fig");
}

/** The figures of the text (`![caption](path)` in Markdown, `\includegraphics{path}` anywhere), in order: of a path
 *  that is a file. A figure labelled has its number before its caption ("Figure 2: …"), from `labels`. */
export function figuresOf(state: EditorState, labels: ReadonlyMap<string, string> = new Map()): Block[] {
  const found: Block[] = [];
  syntaxTree(state).iterate({
    enter(node) {
      if (node.name !== "Image") return;
      const url = node.node.getChild("URL");
      const path = url ? imagePath(state.sliceDoc(url.from, url.to)) : "";
      let caption = /^!\[([^]*)\]/.exec(state.sliceDoc(node.from, url ? url.from : node.to))?.[1]?.trim();
      const label = figureLabel(state, node.node);
      const number = label && labels.get(label);
      if (label && number) caption = caption ? `${refText(label, number)}: ${caption}` : refText(label, number);
      const alone = node.node.parent?.name === "Paragraph";
      const lines = linesOfNode(state, node.node);
      if (isFile(path)) found.push({ ...lines, alone: alone && lines.alone, what: { kind: "image", path, ...(caption ? { caption } : {}) } });
      return false;
    },
  });
  // raw LaTeX: the image under its line, which stays as typed
  const text = state.doc.toString();
  for (const m of text.matchAll(INCLUDE)) {
    const path = m[1].trim();
    const at = m.index ?? 0;
    const lines = { from: state.doc.lineAt(at).from, to: state.doc.lineAt(at + m[0].length).to };
    if (isFile(path)) found.push({ ...lines, alone: false, what: { kind: "image", path } });
  }
  return found.sort((a, b) => a.from - b.from);
}

/** The numbers of the labels and equations (`labelNumbers`). */
type Numbers = { labels: ReadonlyMap<string, string>; equations: ReadonlyMap<number, string> };
const NONE: Numbers = { labels: new Map(), equations: new Map() };

/** The blocks of the text, in order: display math (numbered from `numbers`), figures, the front matter. In a LaTeX
 *  file only the figures, under their lines. */
export function blocksOf(state: EditorState, latex = false, numbers: Numbers = NONE): Block[] {
  if (latex) return figuresOf(state).map((f) => ({ ...f, alone: false }));
  const out: Block[] = figuresOf(state, numbers.labels);
  syntaxTree(state).iterate({
    enter(node) {
      if (node.name === "FrontMatter") {
        const title = /^title:[ \t]*(.*?)[ \t]*$/m.exec(state.sliceDoc(node.from, node.to))?.[1].replace(/^(["'])(.*)\1$/, "$2");
        out.push({ from: node.from, to: state.doc.lineAt(node.to).to, alone: true, what: { kind: "front", ...(title ? { title } : {}) } });
        return false;
      }
      if (!MATH.has(node.name)) return;
      const math = isMathBlock(state, node.node) && mathOf(state, node.node);
      if (math && math.tex) {
        const number = numbers.equations.get(node.from);
        out.push({ ...linesOfNode(state, node.node), what: { kind: "math", tex: math.tex, display: true, ...(number ? { number } : {}) } });
      }
      return false;
    },
  });
  return out.sort((a, b) => a.from - b.from);
}

/** What is read from the whole text, again as it or its tree changes: the section numbers, by where their heading
 *  begins, the numbers of the labels (`labelNumbers`), and the blocks. */
export type Analysis = { numbers: Map<number, string>; labels: ReadonlyMap<string, string>; blocks: Block[] };

/** The analysis of `state`: the headings of the tree as it is (not parsed further: this runs as the text is typed). */
export function analyse(state: EditorState, latex = false): Analysis {
  if (latex) return { numbers: new Map(), labels: new Map(), blocks: blocksOf(state, true) };
  const headings = outline(state, false);
  const numbers = new Map<number, string>();
  for (const h of headings) if (h.number) numbers.set(h.from, h.number);
  const found = labelNumbers(state, headings);
  return { numbers, labels: found.labels, blocks: blocksOf(state, false, found) };
}

/** A citation or cross-reference as it reads (`citationText`): its parts, the cross-references numbered from
 *  `labels`; null when a cross-reference is to a label without a number. */
function citationOf(state: EditorState, node: SyntaxNode, labels: ReadonlyMap<string, string>): Rendering | null {
  const group = state.sliceDoc(node.from, node.from + 1) === "[";
  const parts: CitePart[] = [];
  let at = group ? node.from + 1 : node.from;
  for (let key = node.firstChild; key; key = key.nextSibling) {
    if (key.name !== "CitationKey" && key.name !== "CrossRefKey") continue;
    const name = state.sliceDoc(key.from, key.to);
    const suppress = state.sliceDoc(key.from - 2, key.from) === "-@";
    const start = key.from - (suppress ? 2 : 1);
    parts.push(state.sliceDoc(at, start));
    if (key.name === "CrossRefKey") {
      const number = labels.get(name);
      if (!number) return null;
      parts.push({ ref: refText(name, number) });
    } else parts.push({ key: name, suppress });
    at = key.to;
  }
  parts.push(state.sliceDoc(at, group ? node.to - 1 : node.to));
  return { kind: "cite", parts, group };
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
 *  revealed; the notes' `@initials` are no citations. */
function criticPieces(state: EditorState, from: number, to: number, out: Piece[]) {
  const notes: Span[] = [];
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
      notes.push({ from: note, to: m.to });
    }
  }
  if (!notes.length) return;
  const inNote = (p: Piece) => p.type === "render" && p.what.kind === "cite" && notes.some((n) => n.from <= p.from && p.to <= n.to);
  for (let i = out.length - 1; i >= 0; i--) if (inNote(out[i])) out.splice(i, 1);
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
        if (isMathBlock(state, node.node) || state.doc.lineAt(node.from).number !== state.doc.lineAt(node.to).number) return false;
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
        case "Citation":
        case "CrossRef": {
          const what = citationOf(state, node.node, analysis.labels);
          if (what) out.push({ type: "render", from: node.from, to: node.to, scope: scoped(node), what });
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

/** What is rendered of a rendering, if it has come: HTML (math), a URL (an image), "" (what needs nothing). */
export type Rendered = (what: Rendering) => string | null | undefined;

/** The blocks drawn in place of their lines where the selection is as it is: not revealed, their rendering come. */
export function blocksInPlace(state: EditorState, blocks: Block[], rendered: Rendered = () => ""): Block[] {
  const revealed = revealer(state);
  return blocks.filter((b) => b.alone && !revealed(b.from, b.to) && typeof rendered(b.what) === "string");
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

/** An element rendered, inline or as a block: `what` with what came of it (`value`: HTML of math, the URL of an
 *  image). A click on it puts the cursor in its text. */
class RenderedWidget extends WidgetType {
  readonly what: Rendering;
  readonly value: string;
  readonly block: boolean;
  readonly key: string;
  constructor(what: Rendering, value: string, block: boolean) {
    super();
    [this.what, this.value, this.block, this.key] = [what, value, block, JSON.stringify(what)];
  }
  eq(other: RenderedWidget) {
    return other.key === this.key && other.value === this.value && other.block === this.block;
  }
  get estimatedHeight() {
    return !this.block ? -1 : this.what.kind === "image" ? 200 : 48;
  }
  toDOM(view: EditorView) {
    const dom = document.createElement(this.block ? "div" : "span");
    const what = this.what;
    if (what.kind === "math") {
      dom.className = this.block ? "cm-lp-block" : "cm-lp-inline";
      // the app's rendering (pandoc's MathML of the text typed: the text is escaped there), as the tooltips show it
      dom.innerHTML = this.value;
    } else if (what.kind === "image") {
      dom.className = "cm-figure";
      const img = dom.appendChild(document.createElement("img"));
      img.alt = what.path;
      // the height known once the image is drawn
      img.onload = () => view.requestMeasure();
      img.src = this.value;
      if (what.caption) {
        const caption = dom.appendChild(document.createElement("div"));
        caption.className = "cm-figure-caption";
        caption.textContent = what.caption;
      }
    } else if (what.kind === "cite") {
      dom.className = "cm-lp-cite";
      dom.textContent = this.value;
    } else {
      dom.className = what.title ? "cm-lp-title" : "cm-lp-front";
      dom.textContent = what.title ?? "Front matter";
    }
    if (what.kind === "math" && what.number) {
      const number = dom.appendChild(document.createElement("span"));
      number.className = "cm-lp-eqno";
      number.textContent = `(${what.number})`;
    }
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
  readonly sources: LivePreviewSources;
  constructor(sources: LivePreviewSources) {
    this.sources = sources;
  }

  /** When a key found nowhere is asked again (the reference may be added to the bibliography meanwhile), by key. */
  readonly retry = new Map<string, number>();

  /** What came of `asking` under `key`, if it has; asked if it was not. A citation not found (`again`) is asked again
   *  after a few seconds. */
  one(key: string, asking: () => Promise<string | null>, again = false): string | null | undefined {
    if (this.results.has(key)) {
      const value = this.results.get(key);
      if (value !== null || !again || Date.now() < (this.retry.get(key) ?? 0)) return value;
      this.results.delete(key);
      this.asked.delete(key);
    }
    if (!this.asked.has(key)) {
      this.asked.add(key);
      asking()
        .then((value) => value || null, () => null)
        .then((value) => {
          this.results.set(key, value);
          if (value === null && again) this.retry.set(key, Date.now() + RETRY);
          this.tell();
        });
    }
    return undefined;
  }

  /** The rendering of `what` if it has come (null: it cannot be rendered); asked if it was not. */
  get = (what: Rendering): string | null | undefined => {
    const { hover, image } = this.sources;
    switch (what.kind) {
      case "front":
        return "";
      case "image":
        return this.one(`image:${what.path}`, () => image(what.path));
      case "math": {
        const math = hover?.math;
        if (!math) return null;
        const asking = () => math(what.tex, what.display).then((html) => (html.includes("<math") ? html : null));
        return this.one(`math:${what.display ? "display" : "inline"}:${what.tex}`, asking);
      }
      case "cite": {
        const citation = hover?.citation;
        const label = (key: string) => (citation ? this.one(`cite:${key}`, () => citation(key).then((found) => found?.label ?? null), true) : null);
        return citationText(what, label);
      }
    }
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
/** How long a citation key found nowhere is taken as such before it is asked again, in ms. */
const RETRY = 5000;

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
  for (const r of drawn.rendered) ranges.push(Decoration.replace({ widget: new RenderedWidget(r.what, r.html, false) }).range(r.from, r.to));
  return Decoration.set(ranges, true);
}

/** The decorations of the blocks: rendered in place of their lines, or under them when revealed. */
function blockDecorations(state: EditorState, blocks: Block[], rendered: Rendered): DecorationSet {
  const inPlace = new Set(blocksInPlace(state, blocks, rendered));
  const ranges: Range<Decoration>[] = [];
  for (const b of blocks) {
    const value = rendered(b.what);
    // the front matter revealed is its text alone
    if (typeof value !== "string" || (b.what.kind === "front" && !inPlace.has(b))) continue;
    const widget = new RenderedWidget(b.what, value, true);
    if (inPlace.has(b)) ranges.push(Decoration.replace({ widget, block: true }).range(b.from, b.to));
    else ranges.push(Decoration.widget({ widget, block: true, side: 1 }).range(b.to));
  }
  return Decoration.set(ranges, true);
}

/** Live Preview of a text, its renderings from `renderings`: in a LaTeX file, the blocks (figures) alone. */
function preview(renderings: Renderings, latex: boolean): Extension {
  const rendered: Rendered = renderings.get;
  const field = StateField.define<{ analysis: Analysis; blocks: DecorationSet }>({
    create(state) {
      const analysis = analyse(state, latex);
      return { analysis, blocks: blockDecorations(state, analysis.blocks, rendered) };
    },
    update(value, tr) {
      const parsed = syntaxTree(tr.state) !== syntaxTree(tr.startState);
      const analysis = tr.docChanged || parsed ? analyse(tr.state, latex) : value.analysis;
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
        if (latex) return Decoration.none;
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
  ".cm-lp-block": { position: "relative", padding: "4px 0", textAlign: "center", overflowX: "auto", cursor: "text" },
  ".cm-lp-eqno": { position: "absolute", right: "0", top: "50%", transform: "translateY(-50%)" },
  ".cm-lp-cite": { color: "var(--md-cite, var(--accent))", cursor: "text" },
  ".cm-lp-block math": { display: "block", margin: "0 auto" },
  ".cm-lp-inline": { cursor: "text" },
  // a figure as wide as the column at most, its caption under it
  ".cm-figure": { padding: "6px 0 10px", textAlign: "center" },
  ".cm-figure img": { maxWidth: "100%", maxHeight: "60vh", height: "auto", borderRadius: "3px" },
  ".cm-figure-caption": { fontSize: "0.85em", color: "var(--muted)", marginTop: "0.4em", textAlign: "left" },
  // the front matter: its title, as the preview's first heading
  ".cm-lp-title": { fontSize: "1.6em", fontWeight: "650", lineHeight: "1.3", padding: "0.2em 0 0.4em", color: "var(--md-heading)", cursor: "text" },
  ".cm-lp-front": { fontSize: "0.85em", color: "var(--muted)", cursor: "text" },
});

/** The extension of Live Preview. */
export function livePreview(sources: LivePreviewSources): Extension {
  return preview(new Renderings(sources), sources.latex ?? false);
}
