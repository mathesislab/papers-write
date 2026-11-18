// What the Review menu acts on, wherever the project keeps its suggestions (write.toml `review`): the review's notes
// (ReviewSession) or the marks of CriticMarkup in the text (the editor's). The page holds the one in use as a
// `Reviewer` and the menu asks it alike: Accept Change and Reject Change, Accept All and Reject All, Next and Previous
// Change, Add Comment. The views of the suggestions (Accepted, Original) and Clean Up Review are the notes' only.

import { invoke } from "@tauri-apps/api/core";
import type { FileEditor, Mark, ReviewView } from "papers-write/editor";
import type { ReviewSession } from "$lib/review.svelte";

export interface Reviewer {
  /** where the suggestions are, as the mode control says it */
  readonly where: string;
  /** how many suggestions and comments are open */
  open(): number;
  /** Accept Change and Reject Change: the suggestion at the cursor (the first, where several meet) */
  decideHere(accept: boolean): void;
  /** Accept All and Reject All, asked first */
  decideAll(accept: boolean): Promise<void>;
  /** Next Change and Previous Change, wrapping round */
  step(forward: boolean): void;
  /** why a comment cannot be added now (null: it can) */
  cannotComment(): string | null;
  /** Add Comment… on the text [from, to) */
  comment(from: number, to: number, body: string): Promise<void>;
  /** All Markup, Accepted, Original */
  setView?(view: ReviewView): void;
  /** Clean Up Review */
  cleanUp?(): Promise<void>;
}

type Deps = { editor: () => FileEditor | undefined; say: (what: string) => void };

/** The suggestions in the review's notes. */
export function notesReview(session: ReviewSession, { editor, say }: Deps): Reviewer {
  return {
    where: "in notes",
    open: () => session.items.filter((i) => i.state === "open").length,
    decideHere(accept) {
      const open = (id: string) => session.items.some((s) => s.id === id && s.state === "open");
      const here = editor()?.itemsHere().find((i) => i.kind === "suggestion" && i.id && open(i.id));
      if (!here) return say("No suggestion at the cursor");
      session.act(here.id, accept ? "accept" : "reject").catch(() => {});
    },
    decideAll: (accept) => (accept ? session.acceptAll() : session.rejectAll()).catch(() => {}),
    step: (forward) => session.step(forward),
    cannotComment: () => (session.me ? null : "Save the document first, to comment on it"),
    comment: (from, to, body) => session.comment(from, to, body).catch(() => {}),
    setView: (view) => session.setView(view),
    cleanUp: () => session.cleanUp(),
  };
}

/** The suggestions in the text, as CriticMarkup (`marks`: the editor's, as the text has them now). */
export function textReview(marks: () => Mark[], { editor, say }: Deps): Reviewer {
  const isChange = (m: Mark) => m.kind !== "comment" && m.kind !== "highlight";
  return {
    where: "in the text",
    open: () => marks().length,
    decideHere(accept) {
      const here = editor()?.marksAtCursor().find(isChange);
      if (!here) return say("No suggestion at the cursor");
      editor()?.decideMarkAt(here, accept);
    },
    async decideAll(accept) {
      const n = marks().filter(isChange).length;
      if (!n) return say("No suggestions in the text");
      const verb = accept ? "Accept" : "Reject";
      const message = `${verb} the ${n} suggestion${n === 1 ? "" : "s"} in the text?`;
      if (await invoke<boolean>("review_confirm", { message, ok: `${verb} All` }).catch(() => false)) editor()?.decideAllMarks(accept);
    },
    step(forward) {
      const all = marks();
      if (!all.length) return say("No changes or comments to go to");
      const head = editor()?.view()?.state.selection.main.head ?? 0;
      const mark = forward ? (all.find((m) => m.from > head) ?? all[0]) : (all.findLast((m) => m.to < head) ?? all[all.length - 1]);
      editor()?.select(mark.from, mark.to);
    },
    cannotComment: () => null,
    comment: async (from, to, body) => editor()?.commentInText(from, to, body),
  };
}
