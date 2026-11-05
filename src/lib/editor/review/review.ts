// The review in the editor: what the app adds (`review(handlers)`) and what it calls. The suggestions and comments
// are drawn over the text (overlay.ts); the text itself is always the file's. In suggest mode typing makes the user's
// suggestions (own.ts, suggest.ts).

import { EditorSelection, type EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { modeField, setMode } from "./mode.ts";
import { handlersFacet, overlay, setItems, shownAt, shownOf, type Shown } from "./overlay.ts";
import { stepSuggestions, suggestMode } from "./own.ts";
import type { ReviewHandlers, ReviewItem, ReviewMode } from "./types.ts";

/** The review of the editor: the items drawn over the text, and suggest mode. One instance per editor; it can sit in
 *  a Compartment. */
export function review(handlers: ReviewHandlers): Extension {
  return [handlersFacet.of(handlers), overlay, suggestMode];
}

/** Edit or suggest. */
export function setReviewMode(view: EditorView, mode: ReviewMode) {
  view.dispatch({ effects: setMode.of(mode) });
}

/** The mode: edit without the review. */
export const reviewMode = (state: EditorState): ReviewMode => state.field(modeField, false) ?? "edit";

/** Undo in suggest mode: the last change of the user's suggestions undone (true); false in edit mode, where it is the
 *  text's history. */
export const undoSuggestion = (view: EditorView) => stepSuggestions(view, -1);
export const redoSuggestion = (view: EditorView) => stepSuggestions(view, 1);

/** The items of the review, as the app has them on the editor's current text: they replace those shown. */
export function setReview(view: EditorView, items: ReviewItem[]) {
  view.dispatch({ effects: setItems.of(items) });
}

/** A drawn item as a `ReviewItem`: the app's, where it is now and as it is typed; one of the user's suggestions the
 *  app has not answered yet as made here (`id` empty, `author` the user's slug). */
function itemOf(s: Shown, time = new Date().toISOString()): ReviewItem {
  const replacement = s.kind === "suggestion" ? s.insert : undefined;
  if (s.item) return { ...s.item, at: { from: s.from, to: s.to }, ...(replacement != null ? { replacement } : {}) };
  return {
    id: s.id,
    kind: s.kind,
    author: s.slug,
    record: { id: s.id, kind: s.kind, time, insert: replacement ?? undefined },
    thread: [],
    state: "open",
    at: { from: s.from, to: s.to },
    status: { type: "exact" },
    replacement,
  };
}

/** The items shown whose range holds `pos` (its ends included). */
export function itemsAt(state: EditorState, pos: number): ReviewItem[] {
  return shownAt(state, pos).map((s) => itemOf(s));
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
