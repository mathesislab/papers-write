// What the backend says of git (src-tauri/src/git.rs), as serde writes it.

export type Changed = { path: string; status: string; from?: string };

/** The repository of the document: the branch, how far it is from its upstream (as of the last fetch), the files changed. */
export type GitState = {
  repo: string;
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
