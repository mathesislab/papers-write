// What a window did since the last commit, for the message of the next one (the Commit sheet prefills it): the
// suggestions accepted or rejected, counted by whose; the review cleaned up; edits brought in as suggestions (a branch,
// commits, a file, a Word review). Writing is not logged: a commit of writing only has the message the user writes.

export type Entry = { decision: "accept" | "reject"; who: string; n: number } | { did: string };

/** "Jane Doe", "Jane Doe and Bob", "Jane Doe, Bob and Carl" */
function joined(parts: string[]): string {
  return parts.length > 1 ? `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}` : (parts[0] ?? "");
}

/** The log as one message, its parts in the order first done: "Accept 3 suggestions of Jane Doe and 1 of Bob; review of
 *  jane/edits as suggestions". Empty for an empty log. */
export function message(log: Entry[]): string {
  const order: (string | { decision: "accept" | "reject" })[] = [];
  const counts = { accept: new Map<string, number>(), reject: new Map<string, number>() };
  for (const e of log) {
    if ("did" in e) {
      if (!order.includes(e.did)) order.push(e.did);
      continue;
    }
    if (!e.n) continue;
    const by = counts[e.decision];
    if (!by.size) order.push({ decision: e.decision });
    by.set(e.who, (by.get(e.who) ?? 0) + e.n);
  }
  const parts = order.map((p) => {
    if (typeof p === "string") return p;
    const each = [...counts[p.decision]].map(([who, n], i) => (i === 0 ? `${n} suggestion${n === 1 ? "" : "s"} of ${who}` : `${n} of ${who}`));
    return `${p.decision} ${joined(each)}`;
  });
  const text = parts.join("; ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** The decisions on `items` as entries, one per author: what Accept and Reject log. */
export function decided(decision: "accept" | "reject", items: { author: string }[]): Entry[] {
  const by = new Map<string, number>();
  for (const i of items) by.set(i.author, (by.get(i.author) ?? 0) + 1);
  return [...by].map(([who, n]) => ({ decision, who, n }));
}
