// Live Preview (src/lib/editor/livePreview.ts): what is drawn of a text, and what is hidden of it where the cursor is not.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState, StateEffect, type Extension } from "@codemirror/state";
import { parsed } from "./parsed.ts";
import { hiddenRanges, inlinePieces } from "../src/lib/editor/livePreview.ts";
import { criticReview } from "../src/lib/editor/review/criticMode.ts";
import { shownFacet } from "../src/lib/editor/review/overlay.ts";
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
  assert.deepEqual(some.flatMap((p) => (p.type === "line" ? [state.doc.lineAt(p.at).number] : [])), [3]);
});

test("a figure's caption is the text of its brackets", () => {
  const state = md("![The loop](fig/loop.png){#fig:loop}\n\n![](b.png)\n");
  assert.deepEqual(
    figuresOf(state).map((f) => [f.path, f.caption]),
    [["fig/loop.png", "The loop"], ["b.png", undefined]],
  );
});

/** The texts hidden, the cursor at the `|`. */
const hidden = (marked: string, extensions: Extension[] = []) => {
  let state = md(marked);
  if (extensions.length) state = state.update({ effects: StateEffect.appendConfig.of(extensions) }).state;
  return hiddenRanges(state).map((s) => state.sliceDoc(s.from, s.to));
};

test("the marks hidden off the cursor's line, shown on it", () => {
  const doc = "# Title ##\n\nSome **bold**, *it* and `code`.\n\n> quoted\n\nend";
  assert.deepEqual(hidden(doc), ["# ", " ##", "**", "**", "*", "*", "`", "`", "> "]);
  assert.deepEqual(hidden("# Ti|tle ##\n\nSome **bold**.\n"), ["**", "**"]);
  assert.deepEqual(hidden("# Title\n\nSome **bo|ld**.\n"), ["# "]);
});

test("a selection over lines shows all of them", () => {
  const doc = "*a*\n\n*b*\n\n*c*\n";
  const state = parsed(EditorState.create({ doc, selection: { anchor: 1, head: 6 }, extensions: [markdown({ extensions: paperMarkdown })] }));
  assert.deepEqual(hiddenRanges(state).map((s) => state.sliceDoc(s.from, s.to)), ["*", "*"]);
  assert.equal(hiddenRanges(state)[0].from, doc.indexOf("*c*"));
});

test("links show their text; attributes, escapes and fences are hidden", () => {
  assert.deepEqual(hidden('see [the site](https://example.org "Title") and <https://a.org>\n\nend'), ["[", '](https://example.org "Title")', "<", ">"]);
  assert.deepEqual(hidden("# Intro {#sec:intro}\n\na \\* star\n\n```julia\nx = 1\n```\n\nend"), ["# ", " {#sec:intro}", "\\", "```julia", "```"]);
  // the cursor in the code shows its fences
  assert.deepEqual(hidden("```\nx| = 1\n```\n"), []);
});

test("CriticMarkup delimiters hidden off the line", () => {
  const doc = "an {++insert++} and {~~old~>new~~}{>>@MP: why<<}\n\nend";
  assert.deepEqual(hidden(doc, [criticReview(() => "")]), ["{++", "++}", "{~~", "~>", "~~}{>>", "<<}"]);
  assert.deepEqual(hidden("an {++in|sert++}\n", [criticReview(() => "")]), []);
  // without the review in the text, the marks are text
  assert.deepEqual(hidden(doc), []);
});

test("an element a suggestion or comment of the review is on is shown as typed", () => {
  const doc = "*a* and *b*\n\nend";
  const at = doc.indexOf("b");
  const item = { id: "x-1", kind: "comment" as const, slug: "x", from: at, to: at + 1, insert: null, dim: false, item: null };
  assert.deepEqual(hidden(doc, [shownFacet.of(() => [item])]), ["*", "*"]);
  assert.equal(hiddenRanges(md(doc)).length, 4);
});

test("an element over lines is shown as typed from any of them, and nothing hidden crosses a line", () => {
  const doc = "*a\nb*\n\nend";
  assert.deepEqual(hidden(doc), ["*", "*"]);
  assert.deepEqual(hidden("*a\nb|*\n\nend"), []);
});
