// The sections of a review, for the tabs of the pane (src/lib/sections.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { sections } from "../src/lib/sections.ts";

test("the shallowest level with two headings or more", () => {
  const text = "# Review\n\nintro\n\n## Round 1\n\n### General\n\na\n\n## Round 2\n\nb\n";
  const all = sections(text);
  assert.deepEqual(
    all.map((s) => s.title),
    ["Round 1", "Round 2"],
  );
  assert.equal(text.slice(all[0].from, all[0].to), "## Round 1\n\n### General\n\na\n\n");
  assert.equal(text.slice(all[1].from, all[1].to), "## Round 2\n\nb\n");
});

test("no tabs for a single heading at each level", () => {
  assert.deepEqual(sections("# Review\n\n## Round 1\n\ntext\n"), []);
});

test("headings in fenced code are not headings", () => {
  const text = "## A\n\n```\n## not a heading\n```\n\n## B\n";
  assert.deepEqual(
    sections(text).map((s) => s.title),
    ["A", "B"],
  );
});

test("a section ends at a heading above its level", () => {
  const text = "## A\n\n# Part\n\n## B\n\n## C\n";
  const all = sections(text);
  assert.deepEqual(
    all.map((s) => s.title),
    ["A", "B", "C"],
  );
  assert.equal(text.slice(all[0].from, all[0].to), "## A\n\n");
});
