// Spelling: the system's spell checker on the prose of a Markdown editor (WebKit underlines the misspellings of an
// editable text when `spellcheck` is on). What is not prose (math, code, keys, paths, the front matter, LaTeX commands) is
// marked `spellcheck="false"`, so that WebKit leaves it alone where it honours the attribute on part of a text.
// Autocorrection and capitalisation stay off: the text is what was typed. Opt-in (`Options.spellcheck`).

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

const OFF = Decoration.mark({ attributes: { spellcheck: "false" } });

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
      return Decoration.set(ranges.map((r) => OFF.range(r.from, r.to)), true);
    }
  },
  { decorations: (v) => v.decorations },
);

/** The system's spell checker on the text; not on what is not prose. */
export const spelling: Extension = [
  EditorView.contentAttributes.of({ spellcheck: "true", autocorrect: "off", autocapitalize: "off" }),
  marks,
];
