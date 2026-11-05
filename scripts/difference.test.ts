// The change from one text to another (src/lib/difference.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { difference } from "../src/lib/difference.ts";

const apply = (before: string, c: { from: number; to: number; insert: string }) =>
  before.slice(0, c.from) + c.insert + before.slice(c.to);

test("only the part that differs", () => {
  assert.deepEqual(difference("abc def ghi", "abc XY ghi"), { from: 4, to: 7, insert: "XY" });
  assert.deepEqual(difference("same", "same"), { from: 4, to: 4, insert: "" });
  assert.deepEqual(difference("text", "text and more"), { from: 4, to: 4, insert: " and more" });
  assert.deepEqual(difference("a\tb", "ab"), { from: 1, to: 2, insert: "" });
});

test("applied, it gives the new text, repeats and all", () => {
  for (const [a, b] of [["aaa", "aaaa"], ["abab", "ab"], ["", "x"], ["x", ""], ["hello", "help"]]) {
    assert.equal(apply(a, difference(a, b)), b);
  }
});
