// A project's notes (notes.rs), as their editors use them, beside a text (NotesPane) or in a tab of their own (the
// page of a text): the lines Add Note Here started put at their end, and the link to a place under the cursor
// (`[discussion.md:35](discussion.md#L35)`) followed by a ⌘-click (Ctrl elsewhere).

import type { Text } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

/** The target of the Markdown link at `pos` in `text` that goes to a place in a file of the computer (`#L35`, a line;
 *  `#page=12`, a page of a PDF); none when `pos` is not in such a link. `<target>` is taken without its brackets. */
export function placeLinkAt(text: string, pos: number): string | null {
  const link = /\[[^\]\n]*\]\((?:<([^>\n]+)>|([^)\s]+))\)/g;
  for (let m = link.exec(text); m; m = link.exec(text)) {
    if (m.index > pos) break;
    if (pos > m.index + m[0].length) continue;
    const target = m[1] ?? m[2];
    // a line of a text or a page of a PDF, of a file of the computer
    if (!/#(L\d+|page=\d+)$/.test(target)) return null;
    return /^[a-z][a-z0-9+.-]*:/i.test(target) && !/^[a-z]:[\\/]/i.test(target) ? null : target;
  }
  return null;
}

/** The link under the cursor of `view`, when it goes to a place: the line around the cursor looked at. */
export function placeLinkOf(view: EditorView): string | null {
  const head = view.state.selection.main.head;
  const line = view.state.doc.lineAt(head);
  return placeLinkAt(line.text, head - line.from);
}

/** The change that puts `lines` at the end of the notes `doc`, each on a line of its own after a blank one, with a
 *  space after the last for the note to be typed; none without lines. */
export function appendChange(doc: Text, lines: string[]): { from: number; insert: string } | null {
  if (!lines.length) return null;
  const end = doc.length;
  // a blank line before, however the notes end
  const tail = doc.sliceString(Math.max(0, end - 2), end);
  const before = end === 0 || tail === "\n\n" ? "" : tail.endsWith("\n") ? "\n" : "\n\n";
  return { from: end, insert: `${before}${lines.join("\n")} ` };
}

/** `lines` put at the end of the notes in `view` (`appendChange`), the cursor after them, shown. */
export function appendLines(view: EditorView, lines: string[]) {
  const change = appendChange(view.state.doc, lines);
  if (!change) return;
  view.dispatch({ changes: change, selection: { anchor: change.from + change.insert.length }, scrollIntoView: true });
  view.focus();
}
