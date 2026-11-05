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

const CITE = ["Citation", "CitationKey", "CrossRef", "CrossRefKey", "CiteMark"] as const;

test("bracketed citations, with the key as a node", () => {
  assert.deepEqual(nodes("see [@a; @b, p. 3] there", "Citation", "CitationKey"), ["Citation:[@a; @b, p. 3]", "CitationKey:a", "CitationKey:b"]);
  assert.deepEqual(nodes("[-@a]", "Citation", "CitationKey", "CiteMark"), ["Citation:[-@a]", "CiteMark:[", "CiteMark:-", "CiteMark:@", "CitationKey:a", "CiteMark:]"]);
  assert.deepEqual(nodes("[see @smith:2020, p. 3; also @b-c]", "CitationKey"), ["CitationKey:smith:2020", "CitationKey:b-c"]);
  assert.deepEqual(nodes("[a link](http://x.org) and [@a](http://x.org)", "Citation"), ["Citation:@a"]); // a link, with a citation as its text
  assert.deepEqual(nodes("[no key] and [only @ here]", "Citation"), []);
});

test("citations in the text", () => {
  assert.deepEqual(nodes("@a says so; @b.c. And @d_e:f.", "Citation", "CitationKey"), [
    "Citation:@a",
    "CitationKey:a",
    "Citation:@b.c",
    "CitationKey:b.c",
    "Citation:@d_e:f",
    "CitationKey:d_e:f",
  ]);
});

test("an email address is no citation", () => {
  assert.deepEqual(nodes("write to me@example.org or <me@example.org>", ...CITE), []);
});

test("\\cite commands give their keys", () => {
  assert.deepEqual(nodes("as \\citep{a,b} and \\citet[p. 3]{ c }", "LatexCommand", "CitationKey"), [
    "LatexCommand:\\citep",
    "CitationKey:a",
    "CitationKey:b",
    "LatexCommand:\\citet",
    "CitationKey:c",
  ]);
});

test("cross-references are no citations", () => {
  assert.deepEqual(nodes("in @fig:x and @eq:y, @tbl:z, @sec:w", "CrossRef", "CrossRefKey", "Citation"), [
    "CrossRef:@fig:x",
    "CrossRefKey:fig:x",
    "CrossRef:@eq:y",
    "CrossRefKey:eq:y",
    "CrossRef:@tbl:z",
    "CrossRefKey:tbl:z",
    "CrossRef:@sec:w",
    "CrossRefKey:sec:w",
  ]);
  assert.deepEqual(nodes("@fig says", "Citation", "CitationKey", "CrossRefKey"), ["Citation:@fig", "CitationKey:fig"]);
  assert.deepEqual(nodes("see [@fig:x; @a]", "CrossRefKey", "CitationKey"), ["CrossRefKey:fig:x", "CitationKey:a"]);
});

test("labels in attributes and in LaTeX", () => {
  assert.deepEqual(nodes("![caption](f.png){#fig:x width=50%}", "Attributes", "LabelName"), ["Attributes:{#fig:x width=50%}", "LabelName:fig:x"]);
  assert.deepEqual(nodes("## Methods {#sec:methods .unnumbered}", "LabelName"), ["LabelName:sec:methods"]);
  assert.deepEqual(nodes("$$ a = b $$ {#eq:x}", "DisplayMath", "LabelName"), ["DisplayMath:$$ a = b $$", "LabelName:eq:x"]);
  assert.deepEqual(nodes("$$\na = b\n$$ {#eq:y}\n", "DisplayMath", "LabelName"), ["DisplayMath:$$\na = b\n$$ {#eq:y}", "LabelName:eq:y"]);
  assert.deepEqual(nodes("{.class} and {not attributes #} and {a b}", "Attributes"), []);
  assert.deepEqual(nodes("see \\label{x}, \\ref{x} and \\eqref{eq:y}", "LatexCommand", "LabelName", "CrossRefKey"), [
    "LatexCommand:\\label",
    "LabelName:x",
    "LatexCommand:\\ref",
    "CrossRefKey:x",
    "LatexCommand:\\eqref",
    "CrossRefKey:eq:y",
  ]);
  assert.deepEqual(nodes("\\begin{equation}\\label{eq:z}\nx\n\\end{equation}", "LabelName"), ["LabelName:eq:z"]);
});
