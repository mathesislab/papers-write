// The pure parts of the diagnostics (src/lib/editor/diagnostics.ts): labels not defined, labels defined twice, the keys cited.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdown } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { citationKeys, labelProblems } from "../src/lib/editor/diagnostics.ts";
import { paperMarkdown, tex } from "../src/lib/editor/lang/index.ts";

const make = (doc: string, latex = false) => {
  const state = EditorState.create({ doc, extensions: [latex ? tex : markdown({ extensions: paperMarkdown })] });
  ensureSyntaxTree(state, doc.length, 1000);
  return state;
};
const texts = (doc: string, latex = false) => labelProblems(make(doc, latex)).map((p) => [doc.slice(p.from, p.to), p.message]);

test("a reference to a label the document does not define", () => {
  const doc = "# A {#sec:a}\n\n![Loop](l.png){#fig:loop}\n\nSee @fig:loop, @sec:a and @fig:nope [@eq:gone].\n";
  assert.deepEqual(texts(doc), [
    ["fig:nope", "no label fig:nope in the document"],
    ["eq:gone", "no label eq:gone in the document"],
  ]);
  assert.deepEqual(texts("# A {#sec:a}\n\nSee @sec:a.\n"), []);
});

test("labels in math environments and in LaTeX", () => {
  assert.deepEqual(texts("\\begin{equation}\n x \\label{eq:x}\n\\end{equation}\n\nSee \\ref{eq:x} and \\eqref{eq:y}.\n"), [["eq:y", "no label eq:y in the document"]]);
  assert.deepEqual(texts("\\label{a}\n\\ref{a} \\ref{b,a} % \\ref{c}\n", true), [["b", "no label b in the document"]]);
});

test("a LaTeX file that takes files in cannot say a label is missing", () => {
  assert.deepEqual(texts("\\input{part}\n\\ref{b}\n", true), []);
});

test("a label defined twice is underlined at both", () => {
  const doc = "# A {#sec:a}\n\n# B {#sec:a}\n\n![x](x.png){#fig:x}\n";
  assert.deepEqual(texts(doc), [
    ["sec:a", "label sec:a is defined more than once"],
    ["sec:a", "label sec:a is defined more than once"],
  ]);
  assert.deepEqual(texts("\\label{x} \\label{x}\n", true).length, 2);
});

test("the citation keys, where they are", () => {
  const doc = "As [@a2020; @b2021, p. 3] and @c2022 show, @fig:x and me@example.org; \\cite{d,e}.";
  const state = make(doc);
  assert.deepEqual(
    citationKeys(state).map((k) => k.key),
    ["a2020", "b2021", "c2022", "d", "e"],
  );
  const [first] = citationKeys(state);
  assert.equal(doc.slice(first.from, first.to), "a2020");
  assert.deepEqual(citationKeys(make("\\cite{p,q} \\citep[see]{r}", true)).map((k) => k.key), ["p", "q", "r"]);
});
