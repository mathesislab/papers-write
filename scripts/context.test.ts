// Where the cursor is (src/lib/editor/context.ts): what keeps the symbols typed out, and what the completion offers.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdownLanguage } from "@codemirror/lang-markdown";
import { contextAt, labelsOf, literalAt, nameOptions, refOptions } from "../src/lib/editor/context.ts";
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

const context = (marked: string) => {
  const { tree, pos, before } = at(marked);
  const c = contextAt(tree, pos, before);
  return c && { kind: c.kind, prefix: c.prefix, ...("refKind" in c ? { refKind: c.refKind } : {}) };
};

test("citations: in brackets, in the text, in \\cite", () => {
  assert.deepEqual(context("see [@sch|"), { kind: "citation", prefix: "sch" });
  assert.deepEqual(context("see [-@sch|]"), { kind: "citation", prefix: "sch" });
  assert.deepEqual(context("[@a; @sc|"), { kind: "citation", prefix: "sc" });
  assert.deepEqual(context("see @sc|"), { kind: "citation", prefix: "sc" });
  assert.deepEqual(context("see @|"), { kind: "citation", prefix: "" });
  assert.deepEqual(context("as \\citep{a, sc|"), { kind: "citation", prefix: "sc" });
  assert.deepEqual(context("as \\citet[p. 3]{|"), { kind: "citation", prefix: "" });
  assert.equal(context("write to me@ex|"), null);
  assert.equal(context("plain prose here|"), null);
});

test("cross-references", () => {
  assert.deepEqual(context("in @fig:|"), { kind: "ref", prefix: "", refKind: "fig" });
  assert.deepEqual(context("in @eq:ab|"), { kind: "ref", prefix: "ab", refKind: "eq" });
  assert.deepEqual(context("in \\ref{|"), { kind: "ref", prefix: "" });
  assert.deepEqual(context("in \\eqref{eq:|"), { kind: "ref", prefix: "eq:" });
  const text = "## Methods {#sec:m}\n\n![c](f.png){#fig:a}\n\n$$ x $$ {#eq:b}\n\n\\label{fig:z}\n";
  const labels = labelsOf(parser.parse(text), text);
  assert.deepEqual(labels, ["sec:m", "fig:a", "eq:b", "fig:z"]);
  assert.deepEqual(refOptions(labels, { prefix: "", refKind: "fig" }), [
    { label: "fig:a", insert: "a" },
    { label: "fig:z", insert: "z" },
  ]);
  assert.deepEqual(refOptions(labels, { prefix: "e" }).map((o) => o.label), ["eq:b"]);
});

test("LaTeX names: all in math, the structural ones in the text", () => {
  const names = (marked: string) => {
    const { tree, pos, before } = at(marked);
    const c = contextAt(tree, pos, before);
    return c?.kind === "latex" ? nameOptions(c).map((n) => n.name) : null;
  };
  assert.deepEqual(context("$\\al|$"), { kind: "latex", prefix: "al" });
  assert.ok(names("$\\al|$")?.includes("\\alpha"));
  assert.ok(names("$\\fr|")?.includes("\\frac"));
  assert.ok(names("\\begin{align}\n a \\par|\n\\end{align}")?.includes("\\partial"));
  // in the text: no \alpha, so that Tab gives the symbol
  assert.deepEqual(names("text \\al|"), []);
  assert.deepEqual(names("text \\eta|"), []);
  assert.deepEqual(names("text \\ci|"), ["\\cite", "\\citep", "\\citet", "\\citeauthor", "\\citeyear"]);
  assert.ok(names("text \\begin|")?.includes("\\begin{equation}"));
  assert.equal(names("a line break \\\\|"), null);
});

test("paths", () => {
  assert.deepEqual(context("![x](fig|"), { kind: "path", prefix: "fig" });
  assert.deepEqual(context("\\includegraphics[width=3cm]{figs/|"), { kind: "path", prefix: "figs/" });
  assert.deepEqual(context("\\input{|"), { kind: "path", prefix: "" });
  assert.equal(context("[x](|"), null);
});

test("no completion in the front matter", () => {
  assert.equal(context("---\ntitle: see @sc|\n---\n"), null);
  assert.equal(context("$a @sc|$"), null);
});
