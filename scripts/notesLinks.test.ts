// A project's notes (src/lib/notesLinks.ts): the links to places followed, the lines Add Note Here started put at
// their end.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { Text } from "@codemirror/state";
import { appendChange, placeLinkAt } from "../src/lib/notesLinks.ts";

test("the link to a place under the cursor; none for a web address or outside a link", () => {
  const line = "- 2026-11-23 [discussion.md:35](discussion.md#L35) to check";
  assert.equal(placeLinkAt(line, line.indexOf("discussion")), "discussion.md#L35");
  assert.equal(placeLinkAt(line, line.indexOf("(") + 3), "discussion.md#L35");
  assert.equal(placeLinkAt(line, line.indexOf("to check")), null);
  assert.equal(placeLinkAt("[a b.md:1](<a b.md#L1>)", 2), "a b.md#L1");
  assert.equal(placeLinkAt("[site](https://example.org)", 2), null);
  assert.equal(placeLinkAt("[site](https://example.org/a#L2)", 2), null);
  // a link to a file, not to a place in it: left to what a ⌘-click does in a text
  assert.equal(placeLinkAt("[figure](fig.png)", 2), null);
  assert.equal(placeLinkAt("[pdf p. 3](build/x.pdf#page=3)", 1), "build/x.pdf#page=3");
});

/** The notes after `lines` were put at the end of `text`, and the cursor's place. */
function appended(text: string, lines: string[]) {
  const change = appendChange(Text.of(text.split("\n")), lines);
  if (!change) return { text, at: -1 };
  return { text: text.slice(0, change.from) + change.insert, at: change.from + change.insert.length };
}

test("the lines started at the end, after a blank line, the cursor after them", () => {
  const line = "- 2026-11-23 [a.md:2](a.md#L2)";
  assert.deepEqual(appended("# Notes\n\n", [line]), { text: `# Notes\n\n${line} `, at: 9 + line.length + 1 });
  assert.equal(appended("# Notes\n\nOld.", [line]).text, `# Notes\n\nOld.\n\n${line} `);
  assert.equal(appended("# Notes\n\nOld.\n", [line]).text, `# Notes\n\nOld.\n\n${line} `);
  assert.equal(appended("", [line]).text, `${line} `);
  assert.equal(appended("x", []).text, "x");
});
