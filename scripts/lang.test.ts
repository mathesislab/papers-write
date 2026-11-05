// The academic Markdown of the editor (src/lib/editor/lang): the nodes found in samples.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdownLanguage } from "@codemirror/lang-markdown";
import { paperMarkdown } from "../src/lib/editor/lang/index.ts";

const parser = markdownLanguage.parser.configure(paperMarkdown);

/** The nodes of a text as "Name:text", in document order, for the names asked (all but the plain Markdown ones). */
function nodes(text: string, ...names: string[]): string[] {
  const out: string[] = [];
  parser.parse(text).iterate({
    enter(n) {
      if (names.includes(n.name)) out.push(`${n.name}:${text.slice(n.from, n.to)}`);
    },
  });
  return out;
}

test("front matter at the start, with keys and values", () => {
  const text = "---\ntitle: A paper\nauthor:\n  - Me\n---\n\nbody: not a key\n";
  assert.deepEqual(nodes(text, "FrontMatter", "FrontMatterKey", "FrontMatterValue"), [
    "FrontMatter:---\ntitle: A paper\nauthor:\n  - Me\n---",
    "FrontMatterKey:title",
    "FrontMatterValue:A paper",
    "FrontMatterKey:author",
  ]);
  assert.deepEqual(nodes("---\na: 1\n...\n", "FrontMatter"), ["FrontMatter:---\na: 1\n..."]);
});

test("front matter only at the very start, and only when closed", () => {
  assert.deepEqual(nodes("text\n\n---\na: 1\n---\n", "FrontMatter"), []);
  assert.deepEqual(nodes("\n---\na: 1\n---\n", "FrontMatter"), []);
  assert.deepEqual(nodes("---\na: 1\n", "FrontMatter"), []);
});

test("inline math", () => {
  assert.deepEqual(nodes("so $x^2$ here", "InlineMath", "MathMark", "MathOperator", "MathNumber"), [
    "InlineMath:$x^2$",
    "MathMark:$",
    "MathOperator:^",
    "MathNumber:2",
    "MathMark:$",
  ]);
});

test("pandoc's rules for the dollars", () => {
  assert.deepEqual(nodes("$ x$ and $x $", "InlineMath"), []);
  assert.deepEqual(nodes("$5 and $6", "InlineMath"), []);
  assert.deepEqual(nodes("costs \\$5 and \\$6 or $x$", "InlineMath"), ["InlineMath:$x$"]);
  assert.deepEqual(nodes("$a\\$b$", "InlineMath"), ["InlineMath:$a\\$b$"]);
  assert.deepEqual(nodes("$x$5", "InlineMath"), []);
});

test("commands and braces inside math", () => {
  assert.deepEqual(nodes("$\\frac{a}{b}$", "MathCommand", "MathBrace"), [
    "MathCommand:\\frac",
    "MathBrace:{",
    "MathBrace:}",
    "MathBrace:{",
    "MathBrace:}",
  ]);
});

test("display math in a paragraph, and over lines as a block", () => {
  assert.deepEqual(nodes("$$ \\int $$", "DisplayMath", "MathCommand"), ["DisplayMath:$$ \\int $$", "MathCommand:\\int"]);
  assert.deepEqual(nodes("text $$ x $$ more", "DisplayMath"), ["DisplayMath:$$ x $$"]);
  const text = "before\n\n$$\n\\sum_i x_i\n- y\n$$\n\nafter $y$\n";
  assert.deepEqual(nodes(text, "DisplayMath", "MathCommand", "InlineMath"), ["DisplayMath:$$\n\\sum_i x_i\n- y\n$$", "MathCommand:\\sum", "InlineMath:$y$"]);
});

test("math environments are math, other environments LaTeX", () => {
  const eq = "\\begin{equation}\n  E = mc^2\n\\end{equation}\n";
  assert.deepEqual(nodes(eq, "MathEnvironment", "LatexEnvName", "MathOperator", "MathNumber"), [
    `MathEnvironment:${eq.trimEnd()}`,
    "LatexEnvName:equation",
    "MathOperator:^",
    "MathNumber:2",
    "LatexEnvName:equation",
  ]);
  const fig = "\\begin{figure}\n\\centering\n\\end{figure}\n";
  assert.deepEqual(nodes(fig, "LatexEnvironment", "MathEnvironment", "LatexCommand"), [
    `LatexEnvironment:${fig.trimEnd()}`,
    "LatexCommand:\\begin",
    "LatexCommand:\\centering",
    "LatexCommand:\\end",
  ]);
  assert.deepEqual(nodes("\\begin{align}\na\n", "MathEnvironment", "LatexCommand"), ["LatexCommand:\\begin"]);
});

test("commands in the text", () => {
  assert.deepEqual(nodes("a \\textbf{b} and \\newline* \\\\ \\$", "LatexCommand"), ["LatexCommand:\\textbf", "LatexCommand:\\newline*"]);
});

test("display math and environments in a quote or a list stop with it", () => {
  assert.deepEqual(nodes("> $$\n> x\n> $$\n\nafter", "DisplayMath"), ["DisplayMath:$$\n> x\n> $$"]);
  assert.deepEqual(nodes("> $$\n> x\n\nafter $$ y", "DisplayMath"), ["DisplayMath:$$\n> x"]);
});
