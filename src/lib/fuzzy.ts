// The matching of Go to File (as Sublime's Goto Anything and VS Code's Quick Open match): the letters typed found in
// order in a path, the better the closer they follow one another, at the starts of words, in the file's name rather
// than its folders, and the shorter the path. Words typed apart (a space) are each matched.

/** How well `query` matches `text` (lower case both), letter by letter in order: 0 when it does not; and where. */
export function matchIn(query: string, text: string): { score: number; at: number[] } {
  if (!query) return { score: 1, at: [] };
  let best = { score: 0, at: [] as number[] };
  // each place of the first letter tried as the start: the earliest is not always the best
  for (let start = text.indexOf(query[0]); start !== -1; start = text.indexOf(query[0], start + 1)) {
    const at = [start];
    let i = start;
    for (const c of query.slice(1)) {
      i = text.indexOf(c, i + 1);
      if (i === -1) break;
      at.push(i);
    }
    if (at.length < query.length) break;
    let score = 0;
    at.forEach((p, k) => {
      score += 1;
      if (k > 0 && p === at[k - 1] + 1) score += 5;
      if (p === 0 || "/\\_-. ".includes(text[p - 1])) score += 8;
    });
    if (score > best.score) best = { score, at };
  }
  return best;
}

/** `text` in pieces, those at the places `marked` (counted from `from` of a longer text) marked, for showing what
 *  matched in bold. */
export function pieces(text: string, marked: number[], from = 0): { text: string; on: boolean }[] {
  const out: { text: string; on: boolean }[] = [];
  [...text].forEach((c, i) => {
    const on = marked.includes(i + from);
    if (out.length && out[out.length - 1].on === on) out[out.length - 1].text += c;
    else out.push({ text: c, on });
  });
  return out;
}

/** How well `query` matches the path `relative` (from the project's folder): 0 when it does not; the places matched in
 *  the path, for showing them. */
export function scorePath(query: string, relative: string): { score: number; at: number[] } {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const path = relative.toLowerCase();
  const nameFrom = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")) + 1;
  const name = path.slice(nameFrom);
  let score = 0;
  const at: number[] = [];
  for (const word of words) {
    const inName = matchIn(word, name);
    if (inName.score) {
      score += 2 * inName.score + 20;
      at.push(...inName.at.map((p) => p + nameFrom));
      continue;
    }
    const inPath = matchIn(word, path);
    if (!inPath.score) return { score: 0, at: [] };
    score += inPath.score;
    at.push(...inPath.at);
  }
  return { score: score - relative.length / 100, at: [...new Set(at)].sort((a, b) => a - b) };
}

/** The items whose `relative` path matches `query`, the best first (then the most recently changed); `most` of them. */
export function rank<T extends { relative: string; modified: number }>(items: T[], query: string, most: number): { item: T; at: number[] }[] {
  if (!query.trim()) return [...items].sort((a, b) => b.modified - a.modified).slice(0, most).map((item) => ({ item, at: [] }));
  return items
    .map((item) => ({ item, ...scorePath(query, item.relative) }))
    .filter((m) => m.score > 0)
    .sort((a, b) => b.score - a.score || b.item.modified - a.item.modified)
    .slice(0, most)
    .map(({ item, at }) => ({ item, at }));
}
