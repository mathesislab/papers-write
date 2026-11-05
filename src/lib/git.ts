// What the backend says of git (src-tauri/src/git.rs), as serde writes it.

export type Changed = { path: string; status: string; from?: string };

/** The repository of the document: the branch, how far it is from its upstream (as of the last fetch), the files changed. */
export type GitState = {
  repo: string;
  /** the commit HEAD is at */
  head: string;
  branch: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  changed: Changed[];
};

/** The line at the foot of the window: "main · 3 changed · ↑1 ↓2". */
export function summary(s: GitState): string {
  const parts = [s.branch];
  if (s.changed.length) parts.push(`${s.changed.length} changed`);
  const away = [s.ahead ? `↑${s.ahead}` : "", s.behind ? `↓${s.behind}` : ""].filter(Boolean).join(" ");
  if (away) parts.push(away);
  return parts.join(" · ");
}

/** What edits turned into suggestions wrote: the suggestions added, by whom, and those the review had already. */
export type Added = { added: number; already: number; people: string[] };

/** What was added, in words: "3 suggestions added, of Jane Doe, Bob." */
export function said(made: Added): string {
  const n = made.added;
  const plural = (k: number) => (k === 1 ? "" : "s");
  if (n) return `${n} suggestion${plural(n)} added, of ${made.people.join(", ")}.${made.already ? ` ${made.already} were in the review already.` : ""}`;
  if (made.already) return "Already in the review: nothing added.";
  return "No difference: nothing to suggest.";
}
