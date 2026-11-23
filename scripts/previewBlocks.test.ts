// The blocks of a text as its preview shows them (src/lib/previewBlocks.ts), as pandoc's Markdown cuts them; the
// counts below are those of pandoc 3 (`pandoc -f markdown -t html5`) on the same texts.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { commonmarkLanguage } from "@codemirror/lang-markdown";
import { Text } from "@codemirror/state";
import { paperMarkdown } from "../src/lib/editor/lang/index.ts";
import { blockIndexAt, previewBlocks } from "../src/lib/previewBlocks.ts";

const parser = commonmarkLanguage.parser.configure(paperMarkdown);

/** The blocks of a text, as their texts. */
function blocks(text: string): string[] {
  return previewBlocks(parser.parse(text), Text.of(text.split("\n"))).map((b) => text.slice(b.from, b.to));
}

test("blocks apart, a blank line between them; comments, front matter, link definitions not shown", () => {
  const text = "---\ntitle: T\n---\n\n# One\n\nA paragraph.\n\n<!-- a note -->\n\n[x]: http://x\n\nAnother.\n";
  assert.deepEqual(blocks(text), ["# One", "A paragraph.", "Another."]);
});

test("on the next line of a paragraph, what pandoc keeps in the paragraph: one block", () => {
  for (const between of ["<!-- a note -->", "# Not a heading", "> not a quote", "- not a list", "1. not a list", "***", "$$\nx\n$$", "\\begin{equation}\nx\n\\end{equation}"]) {
    const text = `A line.\n${between}\nNext line.\n\nAfter.\n`;
    assert.deepEqual(blocks(text), [`A line.\n${between}\nNext line.`, "After."], between);
  }
});

test("a paragraph of LaTeX commands alone is not shown; one with text is", () => {
  const text = "Aa.\n\n\\clearpage\n\n\\bibliographystyle{plainnat}\n\n\\renewcommand{\\bibliography}[1]{}\n\n\\noindent Some text.\n";
  assert.deepEqual(blocks(text), ["Aa.", "\\noindent Some text."]);
});

test("fenced code ends a paragraph in pandoc too", () => {
  assert.deepEqual(blocks("A line.\n```\ncode\n```\nNext line.\n"), ["A line.", "```\ncode\n```", "Next line."]);
});

test("a comment after a blank line is apart, and the paragraph right after it is one of its own", () => {
  assert.deepEqual(blocks("A line.\n\n<!-- a note -->\nNext line.\n"), ["A line.", "Next line."]);
});

test("the thesis's case: a comment between two paragraphs without blank lines, the blocks after it in step", () => {
  const text = "To mitigate.\n<!-- a note -->\nMore work.\nWith a constraint.\n\nThe far end.\n\n<!-- one -->\n<!-- two -->\nGlobal mean temperature.\n";
  const all = blocks(text);
  assert.deepEqual(all, ["To mitigate.\n<!-- a note -->\nMore work.\nWith a constraint.", "The far end.", "Global mean temperature."]);
});

test("the block of a place: the one it is in or after", () => {
  const text = "# One\n\nA.\n\n<!-- c -->\n\nB.\n";
  const b = previewBlocks(parser.parse(text), Text.of(text.split("\n")));
  assert.equal(blockIndexAt(b, 0), 0);
  assert.equal(blockIndexAt(b, text.indexOf("A.")), 1);
  assert.equal(blockIndexAt(b, text.indexOf("<!--")), 1);
  assert.equal(blockIndexAt(b, text.indexOf("B.")), 2);
});
