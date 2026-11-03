// The references of a project (a .bib in it, or a papers library), as papers-core finds them for the References
// panel of the review pane (⇧⌘R), and the text of a reference put into the review: a citation, the full reference, a
// citation key (the types, papers-core's, in types.ts).

import type { Item } from "./types";

const LOWER = /^\p{Ll}/u;
const SUFFIX = /^(?:Jr|Sr|II|III|IV)\.?$/;
const NO_YEAR = "n.d.";

/** A display name, "Given Family", in its given and family names: the family name is the last word with the
 *  particles before it ("van den Broeke", "de Boer"), a suffix ("Jr.") left out of it. */
export function splitName(display: string): { given: string; family: string } {
  const words = display.trim().split(/\s+/).filter(Boolean);
  let end = words.length;
  if (end > 1 && SUFFIX.test(words[end - 1])) end--;
  let start = Math.max(end - 1, 0);
  while (start > 0 && LOWER.test(words[start - 1])) start--;
  return { given: words.slice(0, start).join(" "), family: words.slice(start, end).join(" ") };
}

/** The family name of a display name, with its particle. */
export const familyName = (display: string) => splitName(display).family;

/** The initials of given names, each with its stop: "Jan-Hendrik" → "J.-H.", "Anne Marie" → "A. M.", "J.H." →
 *  "J. H.". */
export function initials(given: string): string {
  const letter = (part: string) => `${Array.from(part)[0].toUpperCase()}.`;
  return given
    .split(/\s+/)
    .filter(Boolean)
    .map((word) =>
      word
        .split("-")
        .map((piece) => piece.split(".").filter(Boolean).map(letter).join(" "))
        .filter(Boolean)
        .join("-"),
    )
    .join(" ");
}

/** The year of an item, or "n.d.". */
const yearOf = (item: Item) => (item.year === null ? NO_YEAR : String(item.year));

/** A citation in the text: "Feldmann and Levermann (2015)", "Robinson et al. (2012)", "Robinson (2012)". An item
 *  without authors is cited by its title. */
export function citation(item: Item): string {
  const families = item.authors.map(familyName);
  const who =
    families.length === 0
      ? item.title
      : families.length === 1
        ? families[0]
        : families.length === 2
          ? `${families[0]} and ${families[1]}`
          : `${families[0]} et al.`;
  return `${who} (${yearOf(item)})`;
}

/** The full reference, as Copernicus sets it: "Feldmann, J. and Levermann, A.: Title, Journal, https://doi.org/…,
 *  2015."; the parts an item lacks are left out. */
export function fullReference(item: Item): string {
  const names = item.authors.map((author) => {
    const { given, family } = splitName(author);
    const letters = initials(given);
    return letters ? `${family}, ${letters}` : family;
  });
  const authors =
    names.length <= 2 ? names.join(" and ") : `${names.slice(0, -1).join(", ")}, and ${names[names.length - 1]}`;
  const doi = item.doi?.replace(/^(?:https?:\/\/(?:dx\.)?doi\.org\/|doi:\s*)/i, "").trim();
  const parts = [
    item.title.trim().replace(/\.+$/, ""),
    (item.container ?? item.server)?.trim(),
    doi ? `https://doi.org/${doi}` : "",
    yearOf(item),
  ].filter(Boolean);
  const text = `${authors ? `${authors}: ` : ""}${parts.join(", ")}`;
  return text.endsWith(".") ? text : `${text}.`;
}

/** The citation key as Pandoc cites it: `[@key]`. */
export const pandoc = (item: Item) => `[@${item.key}]`;

/** The citation key as LaTeX cites it: `\cite{key}`. */
export const latex = (item: Item) => `\\cite{${item.key}}`;
