// Edit or suggest: what typing does. Apart from the rest of the review so that the editor's own typing (the symbols,
// the completion) can ask without taking it all in.

import { StateEffect, StateField, type EditorState } from "@codemirror/state";
import type { ReviewMode } from "./types.ts";

export const setMode = StateEffect.define<ReviewMode>();

export const modeField = StateField.define<ReviewMode>({
  create: () => "edit",
  update(mode, tr) {
    for (const e of tr.effects) if (e.is(setMode)) mode = e.value;
    return mode;
  },
});

/** Whether typing makes suggestions, the text left as it is. What is typed then stays as typed: the text before the
 *  cursor is not what was typed, so the symbols typed as sequences and the completion do not apply. */
export const suggesting = (state: EditorState) => state.field(modeField, false) === "suggest";
