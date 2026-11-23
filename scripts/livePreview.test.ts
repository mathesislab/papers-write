// Live Preview (src/lib/editor/livePreview.ts): what is drawn of a text, and what is hidden of it where the cursor is not.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";
import { parsed } from "./parsed.ts";
import { inlinePieces } from "../src/lib/editor/livePreview.ts";
import { figuresOf } from "../src/lib/editor/figures.ts";
import { paperMarkdown } from "../src/lib/editor/lang/index.ts";

/** A Markdown state of `doc`, the cursor at its `|` (at the end without one). */
const md = (marked: string) => {
  const at = marked.indexOf("|");
  const doc = marked.replace("|", "");
  const state = EditorState.create({ doc, selection: { anchor: at < 0 ? doc.length : at }, extensions: [markdown({ extensions: paperMarkdown })] });
  return parsed(state);
};

const pieces = (state: EditorState) => inlinePieces(state, 0, state.doc.length);
/** The classes of the lines, by line number. */
const lineClasses = (state: EditorState) =>
  pieces(state).flatMap((p) => (p.type === "line" ? [[state.doc.lineAt(p.at).number, p.cls]] : []));

test("headings, quotes and code are styled by line, inline code as a range", () => {
  const state = md("# Title\n\ntext `x`\n\n> a quote\n> on\n\n```\ncode\n```\n\nSetext\n------\n");
  assert.deepEqual(lineClasses(state), [
    [1, "cm-lp-h1"],
    [5, "cm-lp-quote"],
    [6, "cm-lp-quote"],
    [8, "cm-lp-codeblock"],
    [9, "cm-lp-codeblock"],
    [10, "cm-lp-codeblock"],
    [12, "cm-lp-h2"],
  ]);
  const code = pieces(state).find((p) => p.type === "mark");
  assert.ok(code?.type === "mark" && state.sliceDoc(code.from, code.to) === "`x`");
});

test("section numbers before the headings, as the outline has them", () => {
  const state = md("# A {#sec:a}\n\n## B\n\n# C\n");
  const numbers = pieces(state).flatMap((p) => (p.type === "number" ? [[state.doc.lineAt(p.at).number, p.text]] : []));
  assert.deepEqual(numbers, [[1, "1"], [3, "1.1"], [5, "2"]]);
  assert.ok(!pieces(md("# A\n\n## B\n")).some((p) => p.type === "number"));
});

test("only the range asked for", () => {
  const state = md("# A\n\n# B\n");
  const some = inlinePieces(state, 5, state.doc.length);
  assert.deepEqual(some.map((p) => (p.type === "line" ? state.doc.lineAt(p.at).number : 0)), [3]);
});

test("a figure's caption is the text of its brackets", () => {
  const state = md("![The loop](fig/loop.png){#fig:loop}\n\n![](b.png)\n");
  assert.deepEqual(
    figuresOf(state).map((f) => [f.path, f.caption]),
    [["fig/loop.png", "The loop"], ["b.png", undefined]],
  );
});
