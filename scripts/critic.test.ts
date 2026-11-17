// CriticMarkup in the text: the marks read, accepted and rejected, and suggest mode's typing written as marks
// (src/lib/editor/review/critic.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { acceptMark, noteOf, parseCritic, rejectMark, resolveAll, suggestIn, type Typed } from "../src/lib/editor/review/critic.ts";

/** `typed` applied to `text`: the text after it and where the cursor is (`|`), or "blocked". */
function type(text: string, typed: Typed, initials = ""): string {
  const s = suggestIn(text, parseCritic(text), typed, initials);
  if ("blocked" in s) return "blocked";
  const after = text.slice(0, s.change.from) + s.change.insert + text.slice(s.change.to);
  return after.slice(0, s.cursor) + "|" + after.slice(s.cursor);
}

/** The keys typed one after the other at the cursor (`|` in `text`), as an editor sends them. */
function keys(text: string, initials: string, ...steps: (string | "⌫" | "⌦")[]): string {
  let at = text.indexOf("|");
  text = text.replace("|", "");
  for (const k of steps) {
    const typed: Typed = k === "⌫" ? { type: "back", from: at - 1, to: at } : k === "⌦" ? { type: "forward", from: at, to: at + 1 } : { type: "replace", from: at, to: at, insert: k };
    const s = suggestIn(text, parseCritic(text), typed, initials);
    if ("blocked" in s) return "blocked";
    text = text.slice(0, s.change.from) + s.change.insert + text.slice(s.change.to);
    at = s.cursor;
  }
  return text.slice(0, at) + "|" + text.slice(at);
}

test("the marks are read with their parts, a note after a mark is its own, with its author", () => {
  const text = "a {++new++}{>>@MP<<} b {--old--} c {~~x~>y~~}{>>@Jane Doe: why<<} d {==this==}{>>a note<<} e {>>alone<<}";
  const marks = parseCritic(text);
  assert.deepEqual(
    marks.map((m) => [m.kind, text.slice(m.from, m.to), m.old && text.slice(m.old.from, m.old.to), m.new && text.slice(m.new.from, m.new.to), m.author, m.note]),
    [
      ["insert", "{++new++}{>>@MP<<}", undefined, "new", "MP", ""],
      ["delete", "{--old--}", "old", undefined, undefined, undefined],
      ["replace", "{~~x~>y~~}{>>@Jane Doe: why<<}", "x", "y", "Jane Doe", "why"],
      ["comment", "{==this==}{>>a note<<}", "this", undefined, undefined, "a note"],
      ["comment", "{>>alone<<}", undefined, undefined, undefined, "alone"],
    ],
  );
});

test("a note's author: @XY alone, with a colon, or the first word", () => {
  assert.deepEqual(noteOf("@MP"), { author: "MP", text: "" });
  assert.deepEqual(noteOf("@MP: too long"), { author: "MP", text: "too long" });
  assert.deepEqual(noteOf("@MP too long"), { author: "MP", text: "too long" });
  assert.deepEqual(noteOf(" plain note "), { text: "plain note" });
});

test("accepted and rejected, each kind; a comment resolved leaves its text", () => {
  const cases: [string, string, string][] = [
    ["a {++new++}{>>@MP<<} b", "a new b", "a  b"],
    ["a {--old--} b", "a  b", "a old b"],
    ["a {~~x~>y~~} b", "a y b", "a x b"],
    ["a {==this==}{>>note<<} b", "a this b", "a this b"],
    ["a {>>note<<} b", "a  b", "a  b"],
  ];
  for (const [text, accepted, rejected] of cases) {
    const [m] = parseCritic(text);
    const apply = (c: { from: number; to: number; insert: string }) => text.slice(0, c.from) + c.insert + text.slice(c.to);
    assert.equal(apply(acceptMark(text, m)), accepted, text);
    assert.equal(apply(rejectMark(text, m)), rejected, text);
  }
  assert.equal(resolveAll("x {++a++} {--b--} {~~c~>d~~}", true), "x a  d");
  assert.equal(resolveAll("x {++a++} {--b--} {~~c~>d~~}", false), "x  b c");
});

test("typing at a point is an insertion with the initials, and goes on into it", () => {
  assert.equal(keys("the |line", "MP", "g", "r", "o", "u", "n", "d", "e", "d", " "), "the {++grounded |++}{>>@MP<<}line");
  assert.equal(keys("the |line", "", "a", " "), "the {++a |++}line");
  // a typo in it, taken back
  assert.equal(keys("the |line", "MP", "x", "y", "⌫"), "the {++x|++}{>>@MP<<}line");
  // emptied, it goes
  assert.equal(keys("the |line", "MP", "x", "⌫"), "the |line");
});

test("Backspace and Delete outside the marks make a deletion that grows", () => {
  assert.equal(keys("the grounding| line", "MP", "⌫", "⌫", "⌫"), "the ground|{--ing--}{>>@MP<<} line");
  assert.equal(keys("the |grounding line", "MP", "⌦", "⌦"), "the {--gr--}{>>@MP<<}|ounding line");
});

test("a selection replaced is a replacement, the cursor in what it puts there", () => {
  const text = "the grounding line";
  assert.equal(type(text, { type: "replace", from: 4, to: 13, insert: "calving" }, "MP"), "the {~~grounding~>calving|~~}{>>@MP<<} line");
  // a selection deleted: a deletion, the cursor after it
  assert.equal(type(text, { type: "replace", from: 4, to: 14, insert: "" }, "MP"), "the {--grounding --}{>>@MP<<}|line");
});

test("a replacement emptied of what it puts there becomes a deletion", () => {
  assert.equal(keys("the {~~x~>y|~~}{>>@MP<<} line", "MP", "⌫"), "the |{--x--}{>>@MP<<} line");
});

test("typing into a mark's delimiters or what a deletion takes out is blocked; next to a mark is not", () => {
  const text = "a {--old--} b";
  assert.equal(type(text, { type: "replace", from: 5, to: 5, insert: "x" }), "blocked");
  assert.equal(type(text, { type: "replace", from: 2, to: 2, insert: "x" }), "a {++x|++}{--old--} b");
  assert.equal(type(text, { type: "replace", from: 0, to: 6, insert: "z" }), "blocked");
});

test("in the editor: suggest mode turns typing into marks, edit mode leaves it, blocked typing changes nothing", async () => {
  const { EditorState } = await import("@codemirror/state");
  const { criticReview, criticMarks } = await import("../src/lib/editor/review/criticMode.ts");
  const { setMode } = await import("../src/lib/editor/review/mode.ts");
  let state = EditorState.create({ doc: "the line {--old--} end", extensions: criticReview(() => "MP") });
  // edit mode: as typed
  state = state.update({ changes: { from: 4, insert: "x" }, userEvent: "input.type" }).state;
  assert.equal(state.doc.toString(), "the xline {--old--} end");
  state = state.update({ effects: setMode.of("suggest") }).state;
  state = state.update({ changes: { from: 4, insert: "y" }, selection: { anchor: 5 }, userEvent: "input.type" }).state;
  assert.equal(state.doc.toString(), "the {++y++}{>>@MP<<}xline {--old--} end");
  assert.equal(state.selection.main.head, 8);
  // inside the deletion: not done
  const before = state.doc.toString();
  const at = before.indexOf("old") + 1;
  state = state.update({ changes: { from: at, insert: "z" }, userEvent: "input.type" }).state;
  assert.equal(state.doc.toString(), before);
  assert.equal(criticMarks(state).length, 2);
  // an undo or the app's own edit is not rewritten
  state = state.update({ changes: { from: 0, to: 3, insert: "The" } }).state;
  assert.ok(state.doc.toString().startsWith("The {++y"));
});

test("a comment on a selection highlights it, the note after it with the initials", async () => {
  const { commentOn } = await import("../src/lib/editor/review/critic.ts");
  const text = "the grounding line";
  const apply = (changes: { from: number; to: number; insert: string }[]) => {
    let out = text;
    for (const c of [...changes].sort((a, b) => b.from - a.from)) out = out.slice(0, c.from) + c.insert + out.slice(c.to);
    return out;
  };
  assert.equal(apply(commentOn(4, 13, "which one?", "MP")), "the {==grounding==}{>>@MP: which one?<<} line");
  assert.equal(apply(commentOn(4, 4, "here", "")), "the {>>here<<}grounding line");
  const [m] = parseCritic(apply(commentOn(4, 13, "which one?", "MP")));
  assert.deepEqual([m.kind, m.author, m.note], ["comment", "MP", "which one?"]);
});

test("in the editor: suggest mode's marks are undone as any edit", async () => {
  const { EditorState } = await import("@codemirror/state");
  const { EditorView } = await import("@codemirror/view");
  const { history, undo } = await import("@codemirror/commands");
  const { criticReview } = await import("../src/lib/editor/review/criticMode.ts");
  const { setMode } = await import("../src/lib/editor/review/mode.ts");
  const { undoSuggestion } = await import("../src/lib/editor/review/review.ts");
  let state = EditorState.create({ doc: "the line", extensions: [history(), criticReview(() => "MP")] });
  state = state.update({ effects: setMode.of("suggest") }).state;
  state = state.update({ changes: { from: 4, to: 8 }, userEvent: "delete.backward" }).state;
  assert.equal(state.doc.toString(), "the {--line--}{>>@MP<<}");
  // the notes' history of suggestions is not there: undo is the text's
  const view = { state, dispatch: (tr: unknown) => (state = (tr as { state: typeof state }).state) } as unknown as InstanceType<typeof EditorView>;
  assert.equal(undoSuggestion(view), false);
  undo({ state, dispatch: (tr) => (state = tr.state) });
  assert.equal(state.doc.toString(), "the line");
});
