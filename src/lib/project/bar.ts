// The tiles of the project bar: a project's short name or initials, and the days left to its due date when they are few.

import { daysLeft } from "./due.ts";

/** The letters a short name holds at most. */
export const SHORT = 4;

/** The days left within which a tile carries a badge. */
const SOON = 7;
/** The days left within which the badge is red, not orange. */
const URGENT = 2;

/** The first letters of the first two words of a name, in capitals (`Ice sheet paper` is `IS`). */
export function initials(name: string): string {
  const words = name.split(/[\s_\-.]+/).filter((w) => w !== "");
  return words
    .slice(0, 2)
    .map((w) => Array.from(w)[0].toUpperCase())
    .join("");
}

/** What a tile says: the short name when set (its first four letters), else the initials; the font size that fits it
 *  in the tile, 36px wide. */
export function chip(name: string, short: string | null): { text: string; size: number } {
  const set = Array.from(short?.trim() ?? "").slice(0, SHORT).join("");
  const text = set || initials(name);
  const size = [13, 13, 13, 11.5, 10][Math.min(Array.from(text).length, SHORT)];
  return { text, size };
}

/** The badge of a tile: the days left when the date is within a week (`!` once past), red when it is close. */
export function badge(due: string | null, now = Date.now()): { text: string; urgent: boolean } | null {
  if (!due) return null;
  const days = daysLeft(due, now);
  if (days > SOON) return null;
  return { text: days < 0 ? "!" : String(days), urgent: days <= URGENT };
}
