// A project's notes (notes.rs), as their editors use them, beside a text (NotesPane) or in a tab of their own (the
// page of a text): the lines Add Note Here started put at their end, and the link to a place under the cursor
// (`[discussion.md:35](discussion.md#L35)`) followed by a ⌘-click (Ctrl elsewhere).

import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Text } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

/** A link to a place: its target (`discussion.md#L35`, `build/synopsis.pdf#page=12`) and its title, the words quoted
 *  to find there (none). */
export type Place = { target: string; title: string | null };

/** Whether a link's target is a web address (or any address with a scheme), not a file of the computer. */
export const isAddress = (target: string) => /^[a-z][a-z0-9+.-]*:/i.test(target) && !/^[a-z]:[\\/]/i.test(target);

/** The Markdown link at `pos` in `text`: its target and its title; none when `pos` is not in a link. `<target>` is
 *  taken without its brackets, the title without its quotes. */
export function linkAt(text: string, pos: number): Place | null {
  const link = /\[[^\]\n]*\]\((?:<([^>\n]+)>|([^)\s]+))(?:\s+"((?:[^"\\\n]|\\.)*)")?\)/g;
  for (let m = link.exec(text); m; m = link.exec(text)) {
    if (m.index > pos) break;
    if (pos > m.index + m[0].length) continue;
    return { target: m[1] ?? m[2], title: m[3] === undefined ? null : m[3].replace(/\\(.)/g, "$1") };
  }
  return null;
}

/** The link at `pos` in `text` that goes to a place in a file of the computer (`#L35`, a line; `#page=12`, a page of a
 *  PDF): what Add Note Here writes; none otherwise. */
export function placeLinkAt(text: string, pos: number): Place | null {
  const link = linkAt(text, pos);
  return link && /#(L\d+|page=\d+)$/.test(link.target) && !isAddress(link.target) ? link : null;
}

/** The link under the cursor of `view`, any (⌘-click follows it). */
export function linkOf(view: EditorView): Place | null {
  const head = view.state.selection.main.head;
  const line = view.state.doc.lineAt(head);
  return linkAt(line.text, head - line.from);
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

/** A link followed, from the text at `from` (a ⌘-click, a tooltip's Follow, a click in a preview): a web address in
 *  the browser; a file, from that text's folder, opened in the app at its line or page, at the words of its title
 *  near there (notes.rs `open_place`). What went wrong, or was not found, said by `say`. */
export function followLink(from: string, link: Place, say: (what: string) => void) {
  if (isAddress(link.target)) return void openUrl(link.target).catch((e) => say(String(e)));
  void invoke<string | null>("open_place", { notes: from, target: link.target, quote: link.title })
    .then((said) => said && say(said))
    .catch((e) => say(String(e)));
}
