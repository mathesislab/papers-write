// What the spell checker leaves alone (src/lib/editor/spelling.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdown } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { notProse, wholeLines } from "../src/lib/editor/spelling.ts";
import { paperMarkdown } from "../src/lib/editor/lang/index.ts";

test("math, code, keys and paths are not prose", () => {
  const doc = "---\ntitle: T\n---\n\nWords $x^2$ and `code` cite [@schoof2007] see @fig:a ![alt](a.png) and more.\n\n```\nfenced\n```\n";
  const state = EditorState.create({ doc, extensions: [markdown({ extensions: paperMarkdown })] });
  ensureSyntaxTree(state, doc.length, 1000);
  const skipped = notProse(state, 0, doc.length).map((r) => doc.slice(r.from, r.to));
  assert.deepEqual(skipped.slice(1), ["$x^2$", "`code`", "schoof2007", "fig:a", "a.png", "```\nfenced\n```"]);
  assert.ok(skipped[0].startsWith("---"));
});

test("lines wholly of non-prose are kept out as lines", () => {
  const doc = "---\ntitle: T\n---\n\nWords $x$ here.\n\n$$\na = b\n$$\n\n```\nfenced\n```\n";
  const state = EditorState.create({ doc, extensions: [markdown({ extensions: paperMarkdown })] });
  ensureSyntaxTree(state, doc.length, 1000);
  const lines = wholeLines(state, notProse(state, 0, doc.length)).map((at) => doc.slice(at, doc.indexOf("\n", at)));
  assert.deepEqual(lines, ["---", "title: T", "---", "$$", "a = b", "$$", "```", "fenced", "```"]);
});
