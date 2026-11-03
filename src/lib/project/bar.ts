// The tiles of the project bar: how a project's short name or initials are set in a tile.

/** The letters a short name holds at most. */
export const SHORT = 10;

/** The letters a tile holds on one line, before the name wraps to two. */
const LINE = 7;
/** The width the name has in a tile, in px, and the width of a letter in em (bold, capitals and all). */
const WIDTH = 60;
const LETTER = 0.62;
/** The font sizes of a name, on one line and on two, at most; and at least. */
const SIZE = { one: 15, two: 12, min: 9 };

/** The first letters of the first two words of a name, in capitals (`Ice sheet paper` is `IS`). */
export function initials(name: string): string {
  const words = name.split(/[\s_\-.]+/).filter((w) => w !== "");
  return words
    .slice(0, 2)
    .map((w) => Array.from(w)[0].toUpperCase())
    .join("");
}

/** What a tile says: the short name when set (its first ten letters), else the initials; on one line when short, else
 *  on two, broken at a space, `_` or `-` near the middle if there is one, else in halves; and the font size that fits
 *  the longer line in the tile. */
export function label(name: string, short: string | null): { lines: string[]; size: number } {
  const set = Array.from(short?.trim() ?? "").slice(0, SHORT);
  const letters = set.length ? set : Array.from(initials(name));
  const lines = wrap(letters);
  const longest = Math.max(1, ...lines.map((l) => Array.from(l).length));
  const max = lines.length > 1 ? SIZE.two : SIZE.one;
  return { lines, size: Math.max(SIZE.min, Math.min(max, Math.floor((WIDTH / (LETTER * longest)) * 2) / 2)) };
}

/** The letters of a name as one line, or two when they are many. */
function wrap(letters: string[]): string[] {
  if (letters.length === 0) return [];
  if (letters.length <= LINE) return [letters.join("")];
  const middle = letters.length / 2;
  let at = -1;
  letters.forEach((c, i) => {
    if (i > 0 && i < letters.length - 1 && " _-".includes(c) && (at < 0 || Math.abs(i - middle) < Math.abs(at - middle))) at = i;
  });
  if (at >= 0) {
    // a hyphen stays at the end of the first line
    const lines = [letters.slice(0, letters[at] === "-" ? at + 1 : at), letters.slice(at + 1)].map((l) => l.join("").trim());
    if (lines.every((l) => Array.from(l).length <= LINE)) return lines;
  }
  const half = Math.ceil(letters.length / 2);
  return [letters.slice(0, half).join(""), letters.slice(half).join("")];
}
