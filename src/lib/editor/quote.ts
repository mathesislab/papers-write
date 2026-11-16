// A quote of a paper put into a review (Papers Review's ⌘U and ⌘Y): on a line of its own, never on a line that has
// text (a heading, a comment being written): the cursor's line if it is empty, else the next empty line below it, else
// after the end. A blank line is kept between it and the text above and below, and the cursor goes after it, to write
// what it is about.

import type { EditorState, TransactionSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

/** The change that puts `quote` in, and the cursor after it. */
export function quoteSpec(state: EditorState, quote: string): TransactionSpec {
  const { doc } = state;
  const empty = (n: number) => n >= 1 && n <= doc.lines && !doc.line(n).text.trim();
  let n = doc.lineAt(state.selection.main.head).number;
  while (n <= doc.lines && !empty(n)) n++;
  // no empty line below: after the last, a blank line between
  if (n > doc.lines) {
    const insert = `\n\n${quote} `;
    return { changes: { from: doc.length, insert }, selection: { anchor: doc.length + insert.length } };
  }
  const line = doc.line(n);
  const before = n === 1 || empty(n - 1) ? "" : "\n";
  const after = n === doc.lines || empty(n + 1) ? "" : "\n";
  return {
    changes: { from: line.from, to: line.to, insert: `${before}${quote} ${after}` },
    selection: { anchor: line.from + before.length + quote.length + 1 },
  };
}

/** The quote put in (`quoteSpec`), in view, the keys to the editor. */
export function insertQuote(view: EditorView, quote: string) {
  view.dispatch({ ...quoteSpec(view.state, quote), scrollIntoView: true, userEvent: "input.quote" });
  view.focus();
}
