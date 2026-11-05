// What the spell checker is given and what it leaves alone (src/lib/editor/spelling.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdown } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { languageOf, notProse, proseRanges } from "../src/lib/editor/spelling.ts";
import { paperMarkdown } from "../src/lib/editor/lang/index.ts";

const make = (doc: string) => {
  const state = EditorState.create({ doc, extensions: [markdown({ extensions: paperMarkdown })] });
  ensureSyntaxTree(state, doc.length, 1000);
  return state;
};
const prose = (doc: string) => {
  const state = make(doc);
  return proseRanges(state, 0, doc.length).map((r) => doc.slice(r.from, r.to).trim());
};

test("math, code, keys and paths are not prose", () => {
  const doc = "---\ntitle: T\n---\n\nWords $x^2$ and `code` cite [@schoof2007] see @fig:a ![alt](a.png) and more.\n\n```\nfenced\n```\n";
  const state = make(doc);
  const skipped = notProse(state, 0, doc.length).map((r) => doc.slice(r.from, r.to));
  assert.deepEqual(skipped.slice(1), ["$x^2$", "`code`", "schoof2007", "fig:a", "a.png", "```\nfenced\n```"]);
  assert.ok(skipped[0].startsWith("---"));
});

test("the prose is what is left: words around math, code and keys", () => {
  assert.deepEqual(prose("Plain wrod $\\alpha_{xyzzq} + qwertz$ and `codde` here [@key2020] ok.\n"), ["Plain wrod", "and", "here [@", "] ok."]);
});

test("the front matter, display math, fenced code, comments and link targets are no prose", () => {
  const doc = "---\ntitle: Zzzq\n---\n\nFirst line.\n\n$$\nqwertz = 1\n$$\n\n<!-- hiddn -->\n\nSee [the linkk](http://exampel.org/pagge) text.\n\n```\nfenced wrod\n```\n\n{#fig:abcd}\n";
  const joined = prose(doc).join(" | ");
  assert.ok(joined.includes("First line."));
  assert.ok(joined.includes("the linkk"));
  for (const bad of ["Zzzq", "qwertz", "hiddn", "exampel", "wrod", "abcd"]) assert.ok(!joined.includes(bad), bad);
});

test("the language of the front matter", () => {
  assert.equal(languageOf("---\ntitle: T\nlang: en-GB\n---\n\ntext"), "en-GB");
  assert.equal(languageOf("---\nlang: \"de\"\n---\n"), "de");
  assert.equal(languageOf("---\ntitle: T\n---\n\nlang: fr\n"), undefined);
  assert.equal(languageOf("no front matter"), undefined);
});
