// The code of Julia, Python and R in fences, Quarto's cells among them (src/lib/editor/lang/cells.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { StringStream } from "@codemirror/language";
import { cellLanguage, cellParser } from "../src/lib/editor/lang/cells.ts";

/** The tokens of `line` that have a style: `style:text`. */
function tokens(lang: string, line: string): string[] {
  const parser = cellParser(lang);
  const stream = new StringStream(line, 4, 2);
  const out: string[] = [];
  while (!stream.eol()) {
    const style = parser.token(stream, null);
    if (style) out.push(`${style}:${stream.current()}`);
    stream.start = stream.pos;
  }
  return out;
}

test("a cell's options, comments, strings, numbers and keywords", () => {
  assert.deepEqual(tokens("julia", "#| label: fig-run"), ["meta:#| label: fig-run"]);
  assert.deepEqual(tokens("julia", 'for x in 1:10 # the runs'), ["keyword:for", "keyword:in", "number:1", "number:10", "comment:# the runs"]);
  assert.deepEqual(tokens("julia", 'println("a #b") ; y = x\''), ['string:"a #b"']);
  assert.deepEqual(tokens("python", "def f(s='x'): return 2.5e3"), ["keyword:def", "string:'x'", "keyword:return", "number:2.5e3"]);
  assert.deepEqual(tokens("r", "x <- NULL"), ["keyword:NULL"]);
});

test("the languages of a fence's info", () => {
  assert.ok(cellLanguage("{julia}"));
  assert.ok(cellLanguage("{r echo=FALSE}"));
  assert.ok(cellLanguage("python"));
  assert.equal(cellLanguage("{julia}"), cellLanguage("julia"));
  assert.equal(cellLanguage("haskell"), null);
  assert.equal(cellLanguage(""), null);
});
