// The outline of a document: its headings (Markdown `#`…`######` and underlined ones; `\section`… in a LaTeX file) with
// their level and where they are, for the column beside the text. A pure function of the state, tested without an editor.

import { language, syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

/** A heading: its level (1 is the shallowest the markup has: `#`, `\part`), its text, and where its line begins. */
export type Heading = { level: number; title: string; from: number };

/** The text of a heading as the markup has it, without its marks and its attributes (`{#sec:x}`). */
export const headingTitle = (text: string) =>
  text
    .split("\n")[0]
    .replace(/^#{1,6}[ \t]+/, "")
    .replace(/[ \t]*\{[^}]*\}[ \t]*$/, "")
    .replace(/[ \t]+#+[ \t]*$/, "")
    .trim();

const LEVELS = ["part", "chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph"];
const SECTIONING = /^\s*\\(part|chapter|section|subsection|subsubsection|paragraph|subparagraph)\*?(?:\[[^\]]*\])?\{/;

/** The text of the braces that begin at `open` of `line` (balanced; to the end of the line if they are not closed). */
function braced(line: string, open: number): string {
  let depth = 0;
  for (let i = open; i < line.length; i++) {
    if (line[i] === "\\") i++;
    else if (line[i] === "{") depth++;
    else if (line[i] === "}" && --depth === 0) return line.slice(open + 1, i);
  }
  return line.slice(open + 1);
}

/** The title of a LaTeX heading as the text reads: no labels, the commands that style it taken off. */
const texTitle = (title: string) =>
  title
    .replace(/\\label\{[^}]*\}/g, "")
    .replace(/\\(?:emph|textbf|textit|texttt|textsc|textrm|mbox)\{([^{}]*)\}/g, "$1")
    .replace(/\\(?:\\|,|;|!|ensuremath)\s*/g, " ")
    .replace(/\s+/g, " ")
    .trim();

/** The headings of the document, in order: with the syntax tree of a Markdown file (no heading in code or in a
 *  comment), with the lines of a LaTeX file (the sectioning commands that begin a line, not in a comment). */
export function outline(state: EditorState): Heading[] {
  const found: Heading[] = [];
  if (state.facet(language)?.name === "latex") {
    for (let n = 1; n <= state.doc.lines; n++) {
      const line = state.doc.line(n);
      const m = SECTIONING.exec(line.text);
      if (!m) continue;
      const title = texTitle(braced(line.text, m[0].length - 1));
      found.push({ level: LEVELS.indexOf(m[1]) + 1, title: title || m[1], from: line.from });
    }
    return found;
  }
  syntaxTree(state).iterate({
    enter(node) {
      const m = /^(?:ATXHeading|SetextHeading)([1-6])$/.exec(node.name);
      if (!m) return;
      found.push({ level: Number(m[1]), title: headingTitle(state.sliceDoc(node.from, node.to)) || "Untitled", from: node.from });
    },
  });
  return found;
}

/** The heading the cursor is under: the last that begins at or before `pos`; -1 before the first. */
export function currentHeading(headings: readonly Heading[], pos: number): number {
  let current = -1;
  for (let i = 0; i < headings.length && headings[i].from <= pos; i++) current = i;
  return current;
}
