// Citations (pandoc): `[@key]`, `[-@key]`, `[see @a, p. 3; @b]` and `@key` in the text; cross-references (pandoc-crossref
// and texmark): `@fig:x`, `@eq:x`, `@tbl:x`, `@sec:x`, `@lst:x`; labels in attributes `{#fig:x}`. The key of a citation
// is a node of its own (CitationKey), so is that of a reference (CrossRefKey) and a label (LabelName). `\cite{a,b}`,
// `\label{x}`, `\ref{x}` give the same nodes (see tokens.ts).

import type { Element, InlineParser } from "@lezer/markdown";
import type { Elt } from "./tokens.ts";
import { citationKey, citeMark, crossRefKey, labelName } from "./tags.ts";

export { citationKey, citeMark, crossRefKey, labelName };

/** The key after the `@`: letters, digits, `_`, and inside it `:.#$%&-+?<>~/` followed by a letter or digit. */
const KEY = /[\p{L}\p{N}_]+(?:[:.#$%&\-+?<>~/][\p{L}\p{N}_]+)*/uy;
/** The keys of cross-references begin with their kind. */
const REF = /^(?:fig|eq|tbl|sec|lst):/;
const WORD = /[\p{L}\p{N}_]/u;

const keyNode = (key: string) => (REF.test(key) ? "CrossRefKey" : "CitationKey");

/** The key at the start of `text`, or null. */
function keyAt(text: string, from: number): string | null {
  KEY.lastIndex = from;
  return KEY.exec(text)?.[0] ?? null;
}

/** `[@a; @b, p. 3]`, `[-@a]`, `[see @a]`: not `[@a](url)`. */
export const citationGroup: InlineParser = {
  name: "Citation",
  parse(cx, next, pos) {
    if (next !== 91) return -1;
    const text = cx.slice(pos, cx.end);
    const close = text.slice(1).search(/[\][]/) + 1;
    if (close < 2 || text[close] !== "]" || !text.slice(0, close).includes("@")) return -1;
    if (text[close + 1] === "(" || text[close + 1] === "[") return -1;
    const elt: Elt = (t, f, to, c) => cx.elt(t, f, to, c);
    const children: Element[] = [elt("CiteMark", pos, pos + 1)];
    let found = false;
    let start = 1;
    for (const item of text.slice(1, close).split(";")) {
      // the first `@key` of an item, not in the middle of a word
      const m = /(?<![\p{L}\p{N}_])(-?)@(?=[\p{L}\p{N}_])/u.exec(item);
      const key = m && keyAt(item, m.index + m[0].length);
      if (m && key) {
        found = true;
        const at = pos + start + m.index;
        if (m[1]) children.push(elt("CiteMark", at, at + 1));
        children.push(elt("CiteMark", at + m[1].length, at + m[1].length + 1));
        children.push(elt(keyNode(key), at + m[0].length, at + m[0].length + key.length));
      }
      start += item.length;
      if (start < close) children.push(elt("CiteMark", pos + start, pos + start + 1));
      start++;
    }
    if (!found) return -1;
    children.push(elt("CiteMark", pos + close, pos + close + 1));
    return cx.addElement(elt("Citation", pos, pos + close + 1, children));
  },
  before: "Link",
};

/** `@key` in the text, a citation or (`@fig:x`) a reference; `me@example.org` is neither. */
export const citationInline: InlineParser = {
  name: "CitationKey",
  parse(cx, next, pos) {
    if (next !== 64) return -1;
    if (pos > cx.offset && WORD.test(cx.slice(pos - 1, pos))) return -1;
    const key = keyAt(cx.slice(pos + 1, Math.min(cx.end, pos + 200)), 0);
    if (!key) return -1;
    const node = REF.test(key) ? "CrossRef" : "Citation";
    return cx.addElement(
      cx.elt(node, pos, pos + 1 + key.length, [cx.elt("CiteMark", pos, pos + 1), cx.elt(keyNode(key), pos + 1, pos + 1 + key.length)]),
    );
  },
  before: "Link",
};

const ATTRIBUTE = /\s*(?:#([^\s}]+)|\.[^\s}]+|[\w:-]+=(?:"[^"]*"|'[^']*'|[^\s}"']*))/y;

/** The attributes `{#fig:x .class key=value}` at the start of `text` (which begins at `at`), with a label in them;
 *  the node, or null. */
export function attributes(text: string, at: number, elt: Elt): Element | null {
  const close = text.indexOf("}");
  if (text[0] !== "{" || close < 0 || text.slice(0, close).includes("\n")) return null;
  const inner = text.slice(0, close);
  const labels: Element[] = [];
  let i = 1;
  for (;;) {
    ATTRIBUTE.lastIndex = i;
    const m = ATTRIBUTE.exec(inner);
    if (!m) break;
    if (m[1]) labels.push(elt("LabelName", at + i + m[0].indexOf("#") + 1, at + i + m[0].indexOf("#") + 1 + m[1].length));
    i += m[0].length;
  }
  if (!labels.length || inner.slice(i).trim()) return null;
  return elt("Attributes", at, at + close + 1, labels);
}

/** `{#fig:x}` after a figure, an equation, a heading. */
export const attributesInline: InlineParser = {
  name: "Attributes",
  parse(cx, next, pos) {
    if (next !== 123) return -1;
    const el = attributes(cx.slice(pos, Math.min(cx.end, pos + 300)), pos, (t, f, to, c) => cx.elt(t, f, to, c));
    return el ? cx.addElement(el) : -1;
  },
};
