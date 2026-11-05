// Where the cursor is (src/lib/editor/context.ts): what keeps the symbols typed out, and what the completion offers.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdownLanguage } from "@codemirror/lang-markdown";
import { literalAt } from "../src/lib/editor/context.ts";
import { paperMarkdown } from "../src/lib/editor/lang/index.ts";

const parser = markdownLanguage.parser.configure(paperMarkdown);

/** The text with the cursor at `|`: the text without it, and the cursor, as the tree and the line before it. */
function at(marked: string) {
  const pos = marked.indexOf("|");
  const text = marked.replace("|", "");
  const lineStart = text.lastIndexOf("\n", pos - 1) + 1;
  return { tree: parser.parse(text), pos, before: text.slice(lineStart, pos) };
}

const literal = (marked: string) => {
  const { tree, pos, before } = at(marked);
  return literalAt(tree, pos, before);
};

test("no typed symbols in math", () => {
  assert.equal(literal("so $x|^2$ here"), "math");
  assert.equal(literal("so $x^2$| here"), null);
  assert.equal(literal("$$ a |-> b $$"), "math");
  assert.equal(literal("$$\na |\n$$"), "math");
  assert.equal(literal("\\begin{equation}\n  E |= mc\n\\end{equation}"), "math");
});

test("math not closed yet", () => {
  assert.equal(literal("so $x|"), "math");
  assert.equal(literal("so $x^2$ and $y -|"), "math");
  assert.equal(literal("so $x$ and y -|"), null);
  assert.equal(literal("it costs $5 and -|"), null);
  assert.equal(literal("it costs \\$5 and -|"), null);
});

test("LaTeX environments, the front matter, keys", () => {
  assert.equal(literal("\\begin{figure}\n\\cent|ering\n\\end{figure}"), "latex");
  assert.equal(literal("---\ntitle: a|\n---\n\ntext"), "front");
  assert.equal(literal("---\ntitle: a\n---\n\ntext|"), null);
  assert.equal(literal("see [@a--b|]"), "keys");
  assert.equal(literal("see \\cite{a-|"), "keys");
  assert.equal(literal("![x](a--b|"), "keys");
});

test("prose is no literal", () => {
  assert.equal(literal("a -| b"), null);
  assert.equal(literal("text with CO|_2"), null);
  assert.equal(literal("a $x$ b -|"), null);
});
