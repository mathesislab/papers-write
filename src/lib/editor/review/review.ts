// The review in the editor: what the app adds (`review(handlers)`) and what it calls. The suggestions and comments
// are drawn over the text (overlay.ts); the text itself is always the file's.

import { EditorSelection, type EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { handlersFacet, itemsField, overlay, setItems, shownAt, shownOf, type Shown } from "./overlay.ts";
import type { ReviewHandlers, ReviewItem } from "./types.ts";

/** The review of the editor: the items drawn over the text. One instance per editor; it can sit in a Compartment. */
export function review(handlers: ReviewHandlers): Extension {
  return [handlersFacet.of(handlers), overlay];
}

/** The items of the review, as the app has them on the editor's current text: they replace those shown. */
export function setReview(view: EditorView, items: ReviewItem[]) {
  view.dispatch({ effects: setItems.of(items) });
}

/** A drawn item as a `ReviewItem`: the app's, where it is now. */
function itemOf(s: Shown): ReviewItem | null {
  return s.item && { ...s.item, at: { from: s.from, to: s.to } };
}

/** The items shown whose range holds `pos` (its ends included). */
export function itemsAt(state: EditorState, pos: number): ReviewItem[] {
  return shownAt(state, pos).flatMap((s) => itemOf(s) ?? []);
}

/** The item `id` scrolled to and its range selected; false if it is not shown. */
export function scrollToItem(view: EditorView, id: string): boolean {
  const s = shownOf(view.state).find((s) => s.id === id);
  if (!s) return false;
  view.dispatch({
    selection: EditorSelection.single(s.from, s.to),
    effects: EditorView.scrollIntoView(s.from, { y: "center" }),
  });
  return true;
}
