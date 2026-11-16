// A quote put into a review on a line of its own: the cursor's line if empty, else the next empty one, else after the
// end (src/lib/editor/quote.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { EditorState } from "@codemirror/state";
import { quoteSpec } from "../src/lib/editor/quote.ts";

const Q = 'p5, l12: "the line"';

/** The text after the quote is put in, | where the cursor is before and after. */
function run(text: string): string {
  const at = text.indexOf("|");
  const state = EditorState.create({ doc: text.replace("|", ""), selection: { anchor: at } });
  const after = state.update(quoteSpec(state, Q)).state;
  const out = after.doc.toString();
  const head = after.selection.main.head;
  return out.slice(0, head) + "|" + out.slice(head);
}

test("on the cursor's line when it is empty, a blank line kept above and below", () => {
  assert.equal(run("# Major\n|\nfirst comment"), `# Major\n\n${Q} |\n\nfirst comment`);
  assert.equal(run("one\n\n|\n\ntwo"), `one\n\n${Q} |\n\ntwo`);
  assert.equal(run("|"), `${Q} |`);
});

test("never on a heading: the next empty line below it", () => {
  assert.equal(run("# Ma|jor comments\n\nfirst"), `# Major comments\n\n${Q} |\n\nfirst`);
  assert.equal(run("|# Major\ntext right under\n\nmore"), `# Major\ntext right under\n\n${Q} |\n\nmore`);
});

test("never into a comment being written: after its paragraph", () => {
  assert.equal(run("The meth|od is unclear.\nSee also.\n\n## Minor"), `The method is unclear.\nSee also.\n\n${Q} |\n\n## Minor`);
});

test("no empty line below: after the end, a blank line between", () => {
  assert.equal(run("# Review\nlast| line"), `# Review\nlast line\n\n${Q} |`);
  assert.equal(run("text|\n"), `text\n\n${Q} |`);
});

test("a line of spaces is empty, and the spaces go", () => {
  assert.equal(run("a\n|   \nb"), `a\n\n${Q} |\n\nb`);
});
