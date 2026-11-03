// The tiles of the project bar (src/lib/bar.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { initials, label } from "../src/lib/bar.ts";

test("the first letters of the first two words, in capitals", () => {
  assert.equal(initials("Ice sheet paper"), "IS");
  assert.equal(initials("egusphere-2026-1234"), "E2");
  assert.equal(initials("review"), "R");
  assert.equal(initials("  élan vital "), "ÉV");
  assert.equal(initials(""), "");
});

test("a tile says the short name when set, else the initials, smaller as it grows", () => {
  assert.deepEqual(label("Ice sheet paper", null), { lines: ["IS"], size: 15 });
  assert.deepEqual(label("Ice sheet paper", "  "), { lines: ["IS"], size: 15 });
  assert.deepEqual(label("Ice sheet paper", "GMD1"), { lines: ["GMD1"], size: 15 });
  assert.deepEqual(label("Ice sheet paper", "Review"), { lines: ["Review"], size: 15 });
  assert.deepEqual(label("Ice sheet paper", "Greenld"), { lines: ["Greenld"], size: 13.5 });
  assert.deepEqual(label("", null), { lines: [], size: 15 });
});

test("a long name wraps to two lines, at a break near the middle, else in halves", () => {
  assert.deepEqual(label("x", "Ice sheets"), { lines: ["Ice", "sheets"], size: 12 });
  assert.deepEqual(label("x", "ice-sheets"), { lines: ["ice-", "sheets"], size: 12 });
  assert.deepEqual(label("x", "Greenland1"), { lines: ["Green", "land1"], size: 12 });
  assert.deepEqual(label("x", "ABCDEFGH"), { lines: ["ABCD", "EFGH"], size: 12 });
  // a break that leaves a line too long is not taken
  assert.deepEqual(label("x", "a bcdefghi"), { lines: ["a bcd", "efghi"], size: 12 });
});

test("a short name holds ten letters at most", () => {
  assert.deepEqual(label("x", "Greenland1234"), { lines: ["Green", "land1"], size: 12 });
});
