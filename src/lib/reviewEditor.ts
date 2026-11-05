// The only place the page calls the review API of the editor (papers-write/editor), which another change adds to
// FileEditor: the prop `review`, and the methods `setReview`, `setMode`, `accept` and `scrollTo`. Declared here as
// expected, so the rest of the page checks before that change is merged; when it is, this module can say it with the
// editor's own types (or go).

import type { FileEditor } from "papers-write/editor";
import type { ReviewHandlers, ReviewItem, ReviewMode } from "$lib/editor/review/types";

export type { ReviewHandlers, ReviewItem, ReviewMode };

/** What the editor gets for the review. */
export type ReviewEditor = {
  /** the items to show, carried onto the editor's current text (replaces the earlier ones) */
  setReview(items: ReviewItem[]): void;
  setMode(mode: ReviewMode): void;
  /** puts the replacement of the suggestion in the text, as an edit (which saves the file as usual) */
  accept(item: ReviewItem): void | Promise<void>;
  /** the item `id` brought into view (and selected) in the text */
  scrollTo(id: string): void;
};

/** The editor, as the one that does the review. */
export const reviewing = (editor: FileEditor | undefined) => editor as unknown as ReviewEditor | undefined;

/** The props of FileEditor for the review (`review`), spread where the editor is made. */
export const reviewProps = (review: ReviewHandlers | undefined): Record<string, unknown> => (review ? { review } : {});
