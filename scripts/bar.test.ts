// The tiles of the project bar (src/lib/bar.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { badge, chip, initials } from "../src/lib/bar.ts";

test("the first letters of the first two words, in capitals", () => {
  assert.equal(initials("Ice sheet paper"), "IS");
  assert.equal(initials("egusphere-2026-1234"), "E2");
  assert.equal(initials("review"), "R");
  assert.equal(initials("  élan vital "), "ÉV");
  assert.equal(initials(""), "");
});

test("a tile says the short name when set, else the initials, smaller as it grows", () => {
  assert.deepEqual(chip("Ice sheet paper", null), { text: "IS", size: 13 });
  assert.deepEqual(chip("Ice sheet paper", "  "), { text: "IS", size: 13 });
  assert.deepEqual(chip("Ice sheet paper", "Ice"), { text: "Ice", size: 11.5 });
  assert.deepEqual(chip("Ice sheet paper", "GMD1"), { text: "GMD1", size: 10 });
  assert.deepEqual(chip("Ice sheet paper", "TooLong"), { text: "TooL", size: 10 });
  assert.deepEqual(chip("", null), { text: "", size: 13 });
});

const now = Date.parse("2026-11-10T00:00:00Z");

test("a badge within a week of the due date, red when close, ! once past", () => {
  assert.equal(badge(null, now), null);
  assert.equal(badge("2026-11-30", now), null);
  assert.deepEqual(badge("2026-11-17", now), { text: "7", urgent: false });
  assert.deepEqual(badge("2026-11-12", now), { text: "2", urgent: true });
  assert.deepEqual(badge("2026-11-10", now), { text: "0", urgent: true });
  assert.deepEqual(badge("2026-11-08", now), { text: "!", urgent: true });
});
