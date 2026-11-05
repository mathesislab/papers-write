// The outline of a document: its headings (Markdown `#`…`######` and underlined ones; `\section`… in a LaTeX file) with
// their level and where they are, for the column beside the text. A pure function of the state, tested without an editor.

import { language, syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

/** A heading: its level (1 is the shallowest the markup has: `#`, `\part`), its text, and where its line begins.
 *  `number` is its section number as the preview and the PDF have it ("2", "2.1"), where the document numbers its
 *  sections and the heading is numbered. */
export type Heading = { level: number; title: string; from: number; number?: string };

/** The text of a heading as the markup has it, without its marks and its attributes (`{#sec:x}`). */
export const headingTitle = (text: string) =>
  text
    .split("\n")[0]
    .replace(/^#{1,6}[ \t]+/, "")
    .replace(/[ \t]*\{[^}]*\}[ \t]*$/, "")
    .replace(/[ \t]+#+[ \t]*$/, "")
    .trim();

const LEVELS = ["part", "chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph"];
const SECTIONING = /^\s*\\(part|chapter|section|subsection|subsubsection|paragraph|subparagraph)(\*?)(?:\[[^\]]*\])?\{/;

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

/** The attributes `{…}` closing a Markdown heading line, or "". */
const attributesOf = (text: string) => /\{([^}]*)\}[ \t]*(?:#+[ \t]*)?$/.exec(text.split("\n")[0])?.[1] ?? "";

/** Whether the attributes of a Markdown heading make it unnumbered (`.unnumbered`, `{-}`). */
const isUnnumbered = (attributes: string) => /(?:^|\s)\.unnumbered(?=\s|$)/.test(attributes) || attributes.trim() === "-";

/** Section numbers as the preview (crossref.lua) gives them: a counter per level, the number of a heading its
 *  counters from the shallowest level that has counted (so a document of `##` headings alone numbers 1, 1.1 ...).
 *  `skip` leaves the leading levels not yet counted out of the number (Markdown). */
function numbering(skip: boolean) {
  const counters = new Array<number>(8).fill(0);
  return (level: number) => {
    counters[level]++;
    for (let i = level + 1; i < counters.length; i++) counters[i] = 0;
    let first = 1;
    while (skip && first < level && counters[first] === 0) first++;
    return counters.slice(first, level + 1).join(".");
  };
}

/** The headings of the document, in order: with the syntax tree of a Markdown file (no heading in code or in a
 *  comment), with the lines of a LaTeX file (the sectioning commands that begin a line, not in a comment). */
export function outline(state: EditorState): Heading[] {
  const found: Heading[] = [];
  if (state.facet(language)?.name === "latex") {
    // numbered as LaTeX does: no star; the sections down to the subsubsection (to the subsection with chapters),
    // counted from the chapter if there are any, else from the section
    const chapters = state.doc.toString().search(/^\s*\\chapter\b/m) >= 0;
    const [top, bottom] = chapters ? [2, 4] : [3, 5];
    const next = numbering(false);
    for (let n = 1; n <= state.doc.lines; n++) {
      const line = state.doc.line(n);
      const m = SECTIONING.exec(line.text);
      if (!m) continue;
      const title = texTitle(braced(line.text, m[0].length - 1));
      const level = LEVELS.indexOf(m[1]) + 1;
      const heading: Heading = { level, title: title || m[1], from: line.from };
      if (!m[2] && level >= top && level <= bottom) heading.number = next(level).split(".").slice(top - 1).join(".");
      found.push(heading);
    }
    return found;
  }
  const attributes: string[] = [];
  syntaxTree(state).iterate({
    enter(node) {
      const m = /^(?:ATXHeading|SetextHeading)([1-6])$/.exec(node.name);
      if (!m) return;
      const text = state.sliceDoc(node.from, node.to);
      attributes.push(attributesOf(text));
      found.push({ level: Number(m[1]), title: headingTitle(text) || "Untitled", from: node.from });
    },
  });
  // the preview numbers the sections once any heading is labelled `{#sec:…}`
  if (attributes.some((a) => /(?:^|\s)#sec:/.test(a))) {
    const next = numbering(true);
    found.forEach((h, i) => {
      if (!isUnnumbered(attributes[i])) h.number = next(h.level);
    });
  }
  return found;
}

/** The heading the cursor is under: the last that begins at or before `pos`; -1 before the first. */
export function currentHeading(headings: readonly Heading[], pos: number): number {
  let current = -1;
  for (let i = 0; i < headings.length && headings[i].from <= pos; i++) current = i;
  return current;
}
