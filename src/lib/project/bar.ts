// The tiles of the project bar: a project's initials, and the days left to its due date when they are few.

import { daysLeft } from "./due.ts";

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

/** The badge of a tile: the days left when the date is within a week (`!` once past), red when it is close. */
export function badge(due: string | null, now = Date.now()): { text: string; urgent: boolean } | null {
  if (!due) return null;
  const days = daysLeft(due, now);
  if (days > SOON) return null;
  return { text: days < 0 ? "!" : String(days), urgent: days <= URGENT };
}
