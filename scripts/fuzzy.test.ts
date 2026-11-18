// The matching of Go to File (src/lib/fuzzy.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { matchIn, pieces, rank, scorePath } from "../src/lib/fuzzy.ts";

const files = (...paths: string[]) => paths.map((relative, i) => ({ relative, modified: i }));

test("the letters in order, not necessarily together", () => {
  assert.ok(matchIn("intr", "introduction.md").score > 0);
  assert.ok(matchIn("itd", "introduction.md").score > 0);
  assert.equal(matchIn("tz", "introduction.md").score, 0);
});

test("a match in the file's name before one in its folders", () => {
  const ranked = rank(files("intro/notes.md", "chapters/intro.md"), "intro", 10);
  assert.equal(ranked[0].item.relative, "chapters/intro.md");
});

test("words typed apart each matched, a folder and a name", () => {
  const ranked = rank(files("sections/results.md", "results.md", "sections/methods.md"), "sec res", 10);
  assert.equal(ranked[0].item.relative, "sections/results.md");
  assert.equal(ranked.length, 1);
});

test("letters that follow one another before scattered ones", () => {
  assert.ok(scorePath("disc", "discussion.md").score > scorePath("disc", "data/inputs/scores.csv").score);
});

test("without a query, the most recently changed first", () => {
  const ranked = rank(files("a.md", "b.md", "c.md"), "", 2);
  assert.deepEqual(
    ranked.map((m) => m.item.relative),
    ["c.md", "b.md"],
  );
});

test("the places matched, for showing them", () => {
  assert.deepEqual(scorePath("res", "sections/results.md").at, [9, 10, 11]);
});

test("the places matched in pieces, to show them in bold", () => {
  assert.deepEqual(pieces("paper.md", [0, 1, 6]), [
    { text: "pa", on: true },
    { text: "per.", on: false },
    { text: "m", on: true },
    { text: "d", on: false },
  ]);
  // counted from a longer text: the name after its folder
  assert.deepEqual(pieces("intro.md", [4, 5], 4), [
    { text: "in", on: true },
    { text: "tro.md", on: false },
  ]);
  assert.deepEqual(pieces("abc", []), [{ text: "abc", on: false }]);
});
