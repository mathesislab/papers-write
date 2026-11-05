// How the review shows people and times.

/** The colour of a person: a hue from their slug, the same for everyone who sees it. */
export function colorOf(slug: string): string {
  let h = 0;
  for (const c of slug) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return `hsl(${h % 360} 62% 46%)`;
}

/** "just now", "5 min ago", "2 h ago", "3 d ago", else the date. */
export function ago(time: string, now = Date.now()): string {
  const t = Date.parse(time);
  if (Number.isNaN(t)) return time;
  const s = Math.max(0, (now - t) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  if (s < 30 * 86400) return `${Math.floor(s / 86400)} d ago`;
  return new Date(t).toLocaleDateString();
}

/** The person of a record's id (`<slug>-<n>`): their name if known, else the slug as words. */
export function whoIs(id: string, people: Record<string, string>): string {
  const slug = id.replace(/-\d+$/, "");
  return people[slug] ?? slug.replace(/-/g, " ");
}
