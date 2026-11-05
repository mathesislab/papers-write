// Suggest mode: the user's typing as their own suggestions, the text unchanged (src/lib/editor/review/suggest.ts,
// own.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { ChangeSet, EditorState, Transaction, type TransactionSpec } from "@codemirror/state";
import { diffOwn, mapOwn, suggestEdit, type Edit, type Own } from "../src/lib/editor/review/suggest.ts";
import { setItems } from "../src/lib/editor/review/overlay.ts";
import { setMode } from "../src/lib/editor/review/mode.ts";
import { Sender, ownField } from "../src/lib/editor/review/own.ts";
import { itemsAt, review } from "../src/lib/editor/review/review.ts";
import type { ReviewItem } from "../src/lib/editor/review/types.ts";

const TEXT = "the grounding line retreats rapidly";

/** The edits one after the other from `own`: the suggestions (without keys), the cursor, and the calls the app gets. */
function run(own: Own[], ...edits: Edit[]) {
  let state = { own, nextKey: 1 + Math.max(0, ...own.map((o) => o.key)), cursor: 0 };
  for (const e of edits) state = suggestEdit(state.own, e, TEXT, state.nextKey);
  return {
    own: state.own.map(({ from, to, insert }) => ({ from, to, insert })),
    cursor: state.cursor,
    calls: diffOwn(own, state.own),
  };
}
const type = (at: number, insert: string): Edit => ({ type: "replace", from: at, to: at, insert });
const back = (at: number, size = 1): Edit => ({ type: "back", at, size });
const del = (at: number, size = 1): Edit => ({ type: "forward", at, size });

test("typing at a point: a new insertion, the cursor where it was (drawn after it)", () => {
  const r = run([], type(4, "x"));
  assert.deepEqual(r.own, [{ from: 4, to: 4, insert: "x" }]);
  assert.equal(r.cursor, 4);
  assert.deepEqual(r.calls, { suggest: [{ key: 1, from: 4, to: 4, insert: "x" }], withdraw: [] });
});

test("typing on: one suggestion", () => {
  const r = run([], type(4, "n"), type(4, "e"), type(4, "w"), type(4, " "));
  assert.deepEqual(r.own, [{ from: 4, to: 4, insert: "new " }]);
  assert.equal(r.cursor, 4);
});

test("Backspace in one's own insertion takes from it; emptied, it is withdrawn", () => {
  const own = [{ key: 7, from: 4, to: 4, insert: "ab" }];
  assert.deepEqual(run(own, back(4)).own, [{ from: 4, to: 4, insert: "a" }]);
  const r = run(own, back(4), back(4));
  assert.deepEqual(r.own, []);
  assert.deepEqual(r.calls, { suggest: [], withdraw: [7] });
  // and on: the text before is struck
  assert.deepEqual(run(own, back(4), back(4), back(4)).own, [{ from: 3, to: 4, insert: "" }]);
});

test("⌥⌫ in one's own insertion takes its last word", () => {
  assert.deepEqual(run([{ key: 1, from: 4, to: 4, insert: "very new " }], back(4, 4)).own, [{ from: 4, to: 4, insert: "very " }]);
});

test("Backspace over the text: a deletion that grows left, the cursor at its start", () => {
  const r = run([], back(35), back(34), back(33));
  assert.deepEqual(r.own, [{ from: 32, to: 35, insert: "" }]);
  assert.equal(r.cursor, 32);
  // with the cursor left at its end, it grows all the same
  assert.deepEqual(run([], back(35), back(35)).own, [{ from: 33, to: 35, insert: "" }]);
});

test("Delete: a deletion that grows right, the cursor at its end", () => {
  const r = run([], del(28), del(29), del(30));
  assert.deepEqual(r.own, [{ from: 28, to: 31, insert: "" }]);
  assert.equal(r.cursor, 31);
});

test("a selection replaced: a replacement, the cursor at its end", () => {
  const r = run([], { type: "replace", from: 19, to: 27, insert: "retreat" });
  assert.deepEqual(r.own, [{ from: 19, to: 27, insert: "retreat" }]);
  assert.equal(r.cursor, 27);
});

test("a selection over one's own suggestions takes them in", () => {
  const own = [
    { key: 1, from: 19, to: 27, insert: "retreat" },
    { key: 2, from: 33, to: 35, insert: "" },
  ];
  const r = run(own, { type: "replace", from: 15, to: 34, insert: "X" });
  // the first's text is replaced; the second's struck text goes on past the selection
  assert.deepEqual(r.own, [{ from: 15, to: 35, insert: "X" }]);
  assert.deepEqual(r.calls, { suggest: [{ key: 1, from: 15, to: 35, insert: "X" }], withdraw: [2] });
});

test("typing inside a deletion: a replacement", () => {
  const own = [{ key: 3, from: 28, to: 35, insert: "" }];
  const r = run(own, type(30, "s"), type(30, "o"));
  assert.deepEqual(r.own, [{ from: 28, to: 35, insert: "so" }]);
  assert.equal(r.cursor, 35);
});

test("a deletion growing into one's own suggestion before it: one", () => {
  const own = [{ key: 1, from: 19, to: 27, insert: "retreat" }];
  const r = run(own, back(28), back(19));
  assert.deepEqual(r.own, [{ from: 18, to: 28, insert: "retreat" }]);
  assert.equal(r.cursor, 18);
  // under the key the app knows
  assert.deepEqual(r.calls, { suggest: [{ key: 1, from: 18, to: 28, insert: "retreat" }], withdraw: [] });
});

test("a deletion typed back to the text it struck is withdrawn", () => {
  const r = run([{ key: 4, from: 4, to: 5, insert: "" }], type(5, "g"));
  assert.deepEqual(r.own, []);
  assert.deepEqual(r.calls.withdraw, [4]);
});

test("pending suggestions carried through an edit before them", () => {
  const own = [
    { key: 1, from: 4, to: 4, insert: "new " },
    { key: 2, from: 19, to: 27, insert: "retreat" },
  ];
  const changes = ChangeSet.of({ from: 0, insert: "> " }, TEXT.length);
  assert.deepEqual(
    mapOwn(own, (p, a) => changes.mapPos(p, a)),
    [
      { key: 1, from: 6, to: 6, insert: "new " },
      { key: 2, from: 21, to: 29, insert: "retreat" },
    ],
  );
  // text typed at the ends of a range stays out of it; a range whose text is gone is an insertion there
  const edit = ChangeSet.of([{ from: 19, insert: "<" }, { from: 27, insert: ">" }], TEXT.length);
  assert.deepEqual(mapOwn([own[1]], (p, a) => edit.mapPos(p, a)), [{ key: 2, from: 20, to: 28, insert: "retreat" }]);
  const cut = ChangeSet.of({ from: 15, to: 30 }, TEXT.length);
  assert.deepEqual(mapOwn([own[1]], (p, a) => cut.mapPos(p, a)), [{ key: 2, from: 15, to: 15, insert: "retreat" }]);
});

// In the editor's state: the transaction filter, the app's items, the history.

const handlers = { me: "jane-doe", suggest: async () => "jane-doe-1", withdraw: async () => {} };

function item(id: string, from: number, to: number, replacement: string, author = "Jane Doe"): ReviewItem {
  return {
    id,
    kind: "suggestion",
    author,
    record: { id, kind: "suggestion", time: "2026-11-05T14:00:00Z", insert: replacement },
    thread: [],
    state: "open",
    at: { from, to },
    status: { type: "exact" },
    replacement,
  };
}

function editor(mode: "edit" | "suggest" = "suggest") {
  let state = EditorState.create({ doc: TEXT, extensions: review(handlers) });
  const go = (spec: TransactionSpec) => (state = state.update(spec).state);
  go({ effects: setMode.of(mode) });
  return {
    go,
    get state() {
      return state;
    },
    own: () => state.field(ownField).own.map(({ from, to, insert }) => ({ from, to, insert })),
  };
}

test("in suggest mode typing leaves the text as it is", () => {
  const ed = editor();
  ed.go({ changes: { from: 4, insert: "x" }, selection: { anchor: 5 }, userEvent: "input.type" });
  ed.go({ changes: { from: 4, insert: "y" }, selection: { anchor: 5 }, userEvent: "input.type" });
  assert.equal(ed.state.doc.toString(), TEXT);
  assert.deepEqual(ed.own(), [{ from: 4, to: 4, insert: "xy" }]);
  assert.equal(ed.state.selection.main.head, 4);
  // the app's own changes go through
  ed.go({ changes: { from: 0, insert: "> " } });
  assert.equal(ed.state.doc.toString(), `> ${TEXT}`);
  assert.deepEqual(ed.own(), [{ from: 6, to: 6, insert: "xy" }]);
});

test("Backspace in suggest mode: the deletion, not the text", () => {
  const ed = editor();
  ed.go({ selection: { anchor: 35 } });
  ed.go({ changes: { from: 34, to: 35 }, userEvent: "delete.backward" });
  ed.go({ changes: { from: 33, to: 34 }, userEvent: "delete.backward" });
  assert.equal(ed.state.doc.toString(), TEXT);
  assert.deepEqual(ed.own(), [{ from: 33, to: 35, insert: "" }]);
  assert.equal(ed.state.selection.main.head, 33);
});

test("typing inside someone else's suggestion suggests on the text beneath", () => {
  const ed = editor();
  ed.go({ effects: setItems.of([item("joe-1", 19, 27, "retreat", "Joe")]) });
  ed.go({ selection: { anchor: 22 } });
  ed.go({ changes: { from: 22, insert: "x" }, userEvent: "input.type" });
  assert.deepEqual(ed.own(), [{ from: 22, to: 22, insert: "x" }]);
  assert.deepEqual(
    itemsAt(ed.state, 22).map((i) => [i.id, i.author, i.at, i.replacement]),
    [
      ["joe-1", "Joe", { from: 19, to: 27 }, "retreat"],
      ["", "jane-doe", { from: 22, to: 22 }, "x"],
    ],
  );
});

test("the user's own items from the app are theirs to type into, shown once", () => {
  const ed = editor();
  ed.go({ effects: setItems.of([item("jane-doe-3", 4, 4, "new ")]) });
  assert.deepEqual(ed.own(), [{ from: 4, to: 4, insert: "new " }]);
  ed.go({ selection: { anchor: 4 } });
  ed.go({ changes: { from: 4, insert: "!" }, userEvent: "input.type" });
  assert.deepEqual(ed.own(), [{ from: 4, to: 4, insert: "new !" }]);
  // the app's items again (before the change was sent): the one typed into is kept as typed
  ed.go({ effects: setItems.of([item("jane-doe-3", 4, 4, "new ")]) });
  assert.deepEqual(ed.own(), [{ from: 4, to: 4, insert: "new !" }]);
  const at = itemsAt(ed.state, 4);
  assert.deepEqual(at.map((i) => [i.id, i.replacement]), [["jane-doe-3", "new !"]]);
});

test("the user's item accepted or gone from the app leaves the field", () => {
  const ed = editor();
  ed.go({ effects: setItems.of([item("jane-doe-3", 4, 4, "new ")]) });
  ed.go({ effects: setItems.of([{ ...item("jane-doe-3", 4, 4, "new "), state: "accepted" }]) });
  assert.deepEqual(ed.own(), []);
  assert.deepEqual(itemsAt(ed.state, 4), []);
});

test("undo and redo in suggest mode step through the suggestions, not the text", () => {
  const ed = editor();
  const t0 = Date.now();
  const typed = (insert: string, time: number) =>
    ed.go({ changes: { from: 4, insert }, userEvent: "input.type", annotations: Transaction.time.of(time) });
  ed.go({ selection: { anchor: 4 } });
  // typed together: one step; after a pause, another
  typed("a", t0);
  typed("b", t0 + 100);
  typed("c", t0 + 2000);
  assert.deepEqual(ed.own(), [{ from: 4, to: 4, insert: "abc" }]);
  // the history of the text is not stepped through (what CodeMirror's undo would do is not done)
  const undo = () => ed.go({ changes: { from: 0, to: 1 }, userEvent: "undo" });
  const redo = () => ed.go({ changes: { from: 0, to: 1 }, userEvent: "redo" });
  undo();
  assert.equal(ed.state.doc.toString(), TEXT);
  assert.deepEqual(ed.own(), [{ from: 4, to: 4, insert: "ab" }]);
  undo();
  assert.deepEqual(ed.own(), []);
  undo();
  assert.equal(ed.state.doc.toString(), TEXT);
  redo();
  redo();
  assert.deepEqual(ed.own(), [{ from: 4, to: 4, insert: "abc" }]);
});

test("in edit mode typing edits the text, and the suggestions move with it", () => {
  const ed = editor("edit");
  ed.go({ effects: setItems.of([item("jane-doe-3", 19, 27, "retreat")]) });
  ed.go({ changes: { from: 0, insert: "> " }, userEvent: "input.type" });
  assert.equal(ed.state.doc.toString(), `> ${TEXT}`);
  assert.deepEqual(ed.own(), [{ from: 21, to: 29, insert: "retreat" }]);
});

test("sent to the app 400 ms after the last change; changes while a new one's id is awaited go after, with it", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const calls: string[] = [];
  let answer: (id: string) => void = () => {};
  const app = {
    me: "jane-doe",
    suggest: (s: { id?: string; from: number; to: number; insert: string }) => {
      calls.push(`suggest ${s.id ?? "new"} ${s.from}-${s.to} ${JSON.stringify(s.insert)}`);
      return new Promise<string>((resolve) => (answer = resolve));
    },
    withdraw: async (id: string) => void calls.push(`withdraw ${id}`),
  };
  let state = EditorState.create({ doc: TEXT, extensions: review(app) });
  const sender = new Sender(() => state, (spec) => go(spec));
  const go = (spec: TransactionSpec) => {
    const before = state;
    state = state.update(spec).state;
    sender.update(before);
  };
  const settle = async () => {
    for (let i = 0; i < 5; i++) await Promise.resolve();
  };
  go({ effects: setMode.of("suggest"), selection: { anchor: 4 } });
  go({ changes: { from: 4, insert: "a" }, userEvent: "input.type" });
  t.mock.timers.tick(300);
  go({ changes: { from: 4, insert: "b" }, userEvent: "input.type" });
  t.mock.timers.tick(300);
  assert.deepEqual(calls, []);
  t.mock.timers.tick(100);
  assert.deepEqual(calls, ['suggest new 4-4 "ab"']);
  // typed on while the app writes it: not a second new one
  go({ changes: { from: 4, insert: "c" }, userEvent: "input.type" });
  t.mock.timers.tick(400);
  assert.equal(calls.length, 1);
  answer("jane-doe-1");
  await settle();
  t.mock.timers.tick(400);
  assert.deepEqual(calls, ['suggest new 4-4 "ab"', 'suggest jane-doe-1 4-4 "abc"']);
  // the app gives a new id for the edited one; typed away, that one is withdrawn
  answer("jane-doe-2");
  await settle();
  for (let i = 0; i < 3; i++) go({ changes: { from: 3, to: 4 }, userEvent: "delete.backward" });
  t.mock.timers.tick(400);
  await settle();
  assert.deepEqual(calls.slice(2), ["withdraw jane-doe-2"]);
  assert.deepEqual(state.field(ownField).own, []);
  assert.deepEqual([...state.field(ownField).ids], []);
});
