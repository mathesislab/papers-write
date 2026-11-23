// Live Preview (Settings ▸ Editor, View ▸ Live Preview): the text shown as the preview has it, but where the cursor
// is. Everything of it is in this module, as one extension the editor holds in a Compartment: off, the editor is as
// it was without it. In a LaTeX file it is the figures under their lines alone.
//
// What is drawn is found by pure functions of the state, tested without an editor: `inlinePieces` for a range of the
// text (the classes of the lines and ranges, the section numbers). A ViewPlugin draws those of the lines in view, again
// as the text, the view, the syntax tree or the selection change.

import { syntaxTree } from "@codemirror/language";
import { StateField, type EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { inlineFigures } from "./figures.ts";
import { outline } from "./outline.ts";
import type { HoverSources } from "./types.ts";

/** What Live Preview takes from the app: the images of the figures (a data URI; null: not found), the renderings of
 *  the tooltips (math, citations), and whether the file is LaTeX. */
export type LivePreviewSources = {
  image: (path: string) => Promise<string | null>;
  hover?: HoverSources;
  latex?: boolean;
};

/** What is drawn of the text: a class on a line (at its start), a class on a range, a section number before a
 *  heading. */
export type Piece =
  | { type: "line"; at: number; cls: string }
  | { type: "mark"; from: number; to: number; cls: string }
  | { type: "number"; at: number; text: string };

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

/** A class on each line from `from` to `to` that is in [`start`, `end`]. */
function linesOf(state: EditorState, from: number, to: number, start: number, end: number, cls: string, out: Piece[]) {
  const last = state.doc.lineAt(Math.min(to, end)).number;
  for (let n = state.doc.lineAt(Math.max(from, start)).number; n <= last; n++) out.push({ type: "line", at: state.doc.line(n).from, cls });
}

/** What is drawn of the text from `from` to `to`. */
export function inlinePieces(state: EditorState, from: number, to: number, analysis: Analysis = analyse(state)): Piece[] {
  const out: Piece[] = [];
  syntaxTree(state).iterate({
    from,
    to,
    enter(node) {
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
        case "Blockquote":
          linesOf(state, node.from, node.to, from, to, "cm-lp-quote", out);
          return;
        case "FencedCode":
        case "CodeBlock":
          linesOf(state, node.from, node.to, from, to, "cm-lp-codeblock", out);
          return false;
        case "InlineCode":
          out.push({ type: "mark", from: node.from, to: node.to, cls: "cm-lp-code" });
          return false;
      }
    },
  });
  return out;
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

/** The decorations of the pieces. */
function decorationsOf(pieces: Piece[]): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  for (const p of pieces) {
    if (p.type === "line") ranges.push(Decoration.line({ class: p.cls }).range(p.at));
    else if (p.type === "mark") ranges.push(Decoration.mark({ class: p.cls }).range(p.from, p.to));
    else ranges.push(Decoration.widget({ widget: new NumberWidget(p.text), side: -1 }).range(p.at));
  }
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
      if (u.docChanged || u.viewportChanged || u.selectionSet || syntaxTree(u.state) !== syntaxTree(u.startState)) {
        this.decorations = this.build(u.view);
      }
    }
    build(view: EditorView): DecorationSet {
      const { from, to } = view.viewport;
      return decorationsOf(inlinePieces(view.state, from, to, view.state.field(analysisField)));
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
});

/** The extension of Live Preview. */
export function livePreview(sources: LivePreviewSources): Extension {
  const figures = inlineFigures(sources.image);
  if (sources.latex) return figures;
  return [analysisField, inline, figures, theme];
}
