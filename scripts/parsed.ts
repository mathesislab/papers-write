// A state whose syntax tree is the whole text's, for the tests. `ensureSyntaxTree` parses on, but `syntaxTree(state)` is
// still the tree the state was made with, which its first parse (20 ms; a busy machine or a cold start is slower) can
// leave short; a transaction takes the tree that was parsed on.

import { ensureSyntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

export function parsed(state: EditorState): EditorState {
  ensureSyntaxTree(state, state.doc.length, Infinity);
  return state.update({}).state;
}
