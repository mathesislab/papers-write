// The sections of a review, for the tabs of the pane: its headings at one level, the shallowest with two headings
// or more (## Round 1, ## Round 2 under a # title, say). No name is required of them.

/** A section: its title, and where it lies in the text (from its heading to the next at its level or above). */
export type Section = { title: string; from: number; to: number };

const HEADING = /^(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/;
const FENCE = /^\s*(```|~~~)/;

export function sections(text: string): Section[] {
  const headings: { level: number; title: string; at: number }[] = [];
  let at = 0;
  let fenced = false;
  for (const line of text.split("\n")) {
    if (FENCE.test(line)) fenced = !fenced;
    const m = fenced ? null : HEADING.exec(line);
    if (m) headings.push({ level: m[1].length, title: m[2], at });
    at += line.length + 1;
  }
  const count = new Map<number, number>();
  for (const h of headings) count.set(h.level, (count.get(h.level) ?? 0) + 1);
  const level = [...count.keys()].sort((a, b) => a - b).find((l) => (count.get(l) ?? 0) >= 2);
  if (level === undefined) return [];
  return headings.flatMap((h, i) => {
    if (h.level !== level) return [];
    const next = headings.slice(i + 1).find((n) => n.level <= level);
    return [{ title: h.title || "Untitled", from: h.at, to: next ? next.at : text.length }];
  });
}
