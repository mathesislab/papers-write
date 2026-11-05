// Spelling: the system's spell checker on the prose of a Markdown editor (WebKit underlines the misspellings of an
// editable text when `spellcheck` is on). What is not prose (math, code, keys, paths, the front matter, LaTeX commands) is
// kept out of it in two ways, as WebKit decides by the element that holds the word, not by what is around it:
//   * a mark (`<span spellcheck="false">`) on each such range: honoured for a word typed inside it (WebKit looks for
//     the attribute on the element of the word and its ancestors: `Element::isSpellCheckingEnabled`), but WebKit checks a
//     paragraph (here a line) as one text, so a span inside a checked line is not certain to keep its words from
//     being underlined;
//   * a line wholly of such text (a fenced block, display math, the front matter) as a `spellcheck="false"` line
//     (`.cm-line` is the paragraph WebKit checks), which holds whatever the span does.
// The marks also carry `autocorrect="off"` and `autocapitalize="off"`. The text stays plain editable text.
// Autocorrection and capitalisation stay off everywhere: the text is what was typed. Opt-in (`Options.spellcheck`).

import { syntaxTree } from "@codemirror/language";
import type { EditorState, Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";

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
]);

/** The ranges of `from` to `to` that are not prose, outermost only, in order. */
export function notProse(state: EditorState, from: number, to: number): { from: number; to: number }[] {
  const ranges: { from: number; to: number }[] = [];
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

const OFF = Decoration.mark({ attributes: { spellcheck: "false", autocorrect: "off", autocapitalize: "off" } });
const OFF_LINE = Decoration.line({ attributes: { spellcheck: "false" } });

/** The lines (their starts) wholly in the `ranges`, that are not empty. */
export function wholeLines(state: EditorState, ranges: readonly { from: number; to: number }[]): number[] {
  const starts: number[] = [];
  for (const r of ranges) {
    for (let line = state.doc.lineAt(r.from); ; line = state.doc.line(line.number + 1)) {
      if (line.from > r.to) break;
      if (line.length && line.from >= r.from && line.to <= r.to) starts.push(line.from);
      if (line.number === state.doc.lines) break;
    }
  }
  return starts;
}

const marks = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || syntaxTree(u.state) !== syntaxTree(u.startState)) this.decorations = this.build(u.view);
    }
    build(view: EditorView): DecorationSet {
      const ranges = view.visibleRanges.flatMap((r) => notProse(view.state, r.from, r.to));
      const decorations = ranges.map((r) => OFF.range(r.from, r.to));
      for (const at of wholeLines(view.state, ranges)) decorations.push(OFF_LINE.range(at));
      return Decoration.set(decorations, true);
    }
  },
  { decorations: (v) => v.decorations },
);

/** The system's spell checker on the text; not on what is not prose. */
export const spelling: Extension = [
  EditorView.contentAttributes.of({ spellcheck: "true", autocorrect: "off", autocapitalize: "off" }),
  marks,
];
