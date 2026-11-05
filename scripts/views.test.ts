// The views of the review: what is drawn of the same suggestions in All Markup, Accepted and Original
// (src/lib/editor/review/overlay.ts), and the view as the editor's state keeps it.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { EditorState } from "@codemirror/state";
import { hiddenRanges, setView, viewDecorations, type Shown } from "../src/lib/editor/review/overlay.ts";
import { review, reviewView } from "../src/lib/editor/review/review.ts";
import type { ReviewView } from "../src/lib/editor/review/types.ts";

const sug = (id: string, from: number, to: number, insert: string): Shown => ({ id, kind: "suggestion", slug: "jane-doe", from, to, insert, dim: false, item: null });
const com = (id: string, from: number, to: number): Shown => ({ id, kind: "comment", slug: "jane-doe", from, to, insert: null, dim: false, item: null });

/** What is drawn: [from, to, "mark <class>" | "widget <text> (<class>)"]. */
function drawn(shown: Shown[], view: ReviewView) {
  const out: [number, number, string][] = [];
  const set = viewDecorations(shown, view);
  for (const c = set.iter(); c.value; c.next()) {
    const spec = c.value.spec as { class?: string; widget?: { text?: string; cls?: string } };
    out.push([c.from, c.to, spec.widget ? `widget ${spec.widget.text ?? ""} (${spec.widget.cls?.split(" ")[0]})` : `mark ${spec.class?.split(" ")[0]}`]);
  }
  return out;
}

const shown = [sug("jane-doe-1", 4, 8, "new"), sug("jane-doe-2", 12, 12, " more"), sug("jane-doe-3", 20, 24, ""), com("jane-doe-4", 0, 3)];

test("All Markup: deletions struck, insertions after, comments marked", () => {
  assert.deepEqual(drawn(shown, "markup"), [
    [0, 3, "mark cm-review-comment"],
    [4, 8, "mark cm-review-del"],
    [8, 8, "widget new (cm-review-ins)"],
    [12, 12, "widget  more (cm-review-ins)"],
    [20, 24, "mark cm-review-del"],
  ]);
  assert.deepEqual(hiddenRanges(shown, "markup"), []);
});

test("Accepted: the replaced text hidden, the insertions plain in the author's colour, no strikethrough", () => {
  assert.deepEqual(drawn(shown, "accepted"), [
    [0, 3, "mark cm-review-comment"],
    [8, 8, "widget new (cm-review-acc)"],
    [12, 12, "widget  more (cm-review-acc)"],
  ]);
  assert.deepEqual(hiddenRanges(shown, "accepted"), [
    { from: 4, to: 8 },
    { from: 20, to: 24 },
  ]);
});

test("Accepted: overlapping suggestions hide one merged range", () => {
  assert.deepEqual(hiddenRanges([sug("a-1", 4, 10, "x"), sug("b-1", 8, 14, "y")], "accepted"), [{ from: 4, to: 14 }]);
});

test("Accepted: an outdated or orphaned suggestion is not applied, drawn as in All Markup (dimmed)", () => {
  const stale = { ...sug("jane-doe-5", 30, 34, "old"), dim: true };
  const all = [sug("jane-doe-1", 4, 8, "new"), stale];
  assert.deepEqual(drawn(all, "accepted"), [
    [8, 8, "widget new (cm-review-acc)"],
    [30, 34, "mark cm-review-del"],
    [34, 34, "widget old (cm-review-ins)"],
  ]);
  assert.deepEqual(hiddenRanges(all, "accepted"), [{ from: 4, to: 8 }]);
  assert.deepEqual(drawn([stale], "original"), []);
});

test("Original: no suggestion drawn, comments still marked", () => {
  assert.deepEqual(drawn(shown, "original"), [[0, 3, "mark cm-review-comment"]]);
  assert.deepEqual(hiddenRanges(shown, "original"), []);
});

test("the editor's state keeps the view; the text is never changed by it", () => {
  const handlers = { me: "jane-doe", suggest: async () => "", withdraw: async () => {} };
  let state = EditorState.create({ doc: "the text", extensions: review(handlers) });
  assert.equal(reviewView(state), "markup");
  for (const v of ["accepted", "original", "markup"] as const) {
    state = state.update({ effects: setView.of(v) }).state;
    assert.equal(reviewView(state), v);
    assert.equal(state.doc.toString(), "the text");
  }
  assert.equal(reviewView(EditorState.create({ doc: "x" })), "markup");
});
