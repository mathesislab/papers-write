// The blocks of a Markdown text as its preview shows them, in order: the preview's HTML has no source positions
// (pandoc's Markdown reader keeps none), so a block of the preview is matched to the text by its index among these.
// Everything that goes from one to the other (a click in the preview, the preview following the cursor or the view,
// the text following the preview's tab) takes its blocks from here.
//
// The editor parses CommonMark, pandoc its own Markdown, and the two cut blocks differently: in CommonMark a comment,
// a heading, a quote, a list, a rule or display math can start right after a line of a paragraph, ending it; in
// pandoc's Markdown they cannot (they need a blank line before them), and the lines are part of the paragraph (a
// comment in it is not shown, a `# heading` is text). Fenced code ends a paragraph in both. And a paragraph of LaTeX
// commands alone (`\clearpage`, `\bibliographystyle{plainnat}`) is raw LaTeX to pandoc, not in its HTML.

import type { Text } from "@codemirror/state";
import type { SyntaxNode, Tree } from "@lezer/common";

/** Top-level blocks that are not in pandoc's HTML: front matter, link definitions, comments, raw LaTeX. */
const HIDDEN = new Set(["FrontMatter", "LinkReference", "CommentBlock", "LatexEnvironment"]);

/** LaTeX commands alone, with their arguments: `\name*[...]{...}`, `{...}` holding one level of braces */
const LATEX_ONLY = /^(?:\s*\\[A-Za-z@]+\*?(?:\s*(?:\[[^\]]*\]|\{(?:[^{}]|\{[^{}]*\})*\}))*)+\s*$/;

/** Blocks that end a paragraph in CommonMark when they start on its next line, but not in pandoc's Markdown. */
const NOT_ENDING = new Set([
  "CommentBlock",
  "ATXHeading1",
  "ATXHeading2",
  "ATXHeading3",
  "ATXHeading4",
  "ATXHeading5",
  "ATXHeading6",
  "Blockquote",
  "BulletList",
  "OrderedList",
  "HorizontalRule",
  "DisplayMath",
  "MathEnvironment",
  "LatexEnvironment",
]);

export interface Block {
  from: number;
  to: number;
}

/** The blocks the preview shows, in its order: of the text `doc`, parsed as `tree`. */
export function previewBlocks(tree: Tree, doc: Text): Block[] {
  const line = (pos: number) => doc.lineAt(pos).number;
  const blocks: Block[] = [];
  /** the last block taken was a paragraph (or one that went on as part of it) */
  let paragraph = false;
  for (let n: SyntaxNode | null = tree.topNode.firstChild; n; n = n.nextSibling) {
    const last = blocks[blocks.length - 1];
    const next = last && paragraph && line(n.from) === line(last.to) + 1;
    // starting on the paragraph's next line: pandoc goes on with the paragraph
    if (next && (NOT_ENDING.has(n.name) || n.name === "Paragraph")) {
      last.to = n.to;
      continue;
    }
    const latex = n.name === "Paragraph" && LATEX_ONLY.test(doc.sliceString(n.from, n.to));
    paragraph = n.name === "Paragraph" && !latex;
    if (!HIDDEN.has(n.name) && !latex) blocks.push({ from: n.from, to: n.to });
  }
  return blocks;
}

/** The index of the block (among `previewBlocks`) that `pos` is in or comes after; -1 before the first. */
export function blockIndexAt(blocks: Block[], pos: number): number {
  let index = -1;
  for (const b of blocks) {
    if (b.from > pos) break;
    index++;
  }
  return index;
}
