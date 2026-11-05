// Math by pandoc's rules: inline `$x$` (the opening `$` not followed by a space, the closing one not preceded by a
// space and not followed by a digit; `\$` is no math), display `$$ x $$` in a paragraph, and display math over lines as
// a block. Inside, the commands, `^` `_`, the braces and the numbers are nodes of their own.

import type { BlockParser, Element, InlineParser } from "@lezer/markdown";
import { MATH_ENVS, inside, tokens, type Elt } from "./tokens.ts";
import { mathBrace, mathCommand, mathMark, mathNumber, mathOperator } from "./tags.ts";

export { mathBrace, mathCommand, mathMark, mathNumber, mathOperator };

/** The position in `text` of the next unescaped `$$` from `from`, or -1. */
function closeDisplay(text: string, from: number): number {
  for (let i = from; i < text.length - 1; i++) {
    if (text[i] === "\\") i++;
    else if (text[i] === "$" && text[i + 1] === "$") return i;
  }
  return -1;
}

/** The position in `text` of the `$` that closes the inline math opened before `from`, or -1. */
function closeInline(text: string, from: number): number {
  for (let i = from; i < text.length; i++) {
    if (text[i] === "\\") i++;
    else if (text[i] === "$") {
      const before = text[i - 1];
      return i > from && !/\s/.test(before) && !/\d/.test(text[i + 1] ?? "") ? i : -1;
    }
  }
  return -1;
}

/** `$x$` and `$$ x $$` in the text of a paragraph. */
export const mathInline: InlineParser = {
  name: "Math",
  parse(cx, next, pos) {
    if (next !== 36) return -1;
    const rest = cx.slice(pos, cx.end);
    const elt: Elt = (t, f, to, c) => cx.elt(t, f, to, c);
    if (rest[1] === "$") {
      const close = closeDisplay(rest, 2);
      if (close < 0) return -1;
      const body = tokens(rest.slice(2, close), pos + 2, elt, "math");
      return cx.addElement(
        cx.elt("DisplayMath", pos, pos + close + 2, [cx.elt("MathMark", pos, pos + 2), ...body, cx.elt("MathMark", pos + close, pos + close + 2)]),
      );
    }
    if (rest.length < 2 || /\s/.test(rest[1])) return -1;
    const close = closeInline(rest, 1);
    if (close < 0) return -1;
    const body = tokens(rest.slice(1, close), pos + 1, elt, "math");
    return cx.addElement(cx.elt("InlineMath", pos, pos + close + 1, [cx.elt("MathMark", pos, pos + 1), ...body, cx.elt("MathMark", pos + close, pos + close + 1)]));
  },
  before: "Escape",
};

/** `$$` on a line of its own (or opening a block) up to the line with the closing `$$`; unclosed, up to the blank line. */
export const mathBlock: BlockParser = {
  name: "DisplayMath",
  before: "FencedCode",
  parse(cx, line) {
    const first = line.text.slice(line.pos);
    if (!first.startsWith("$$") || closeDisplay(first, 2) >= 0) return false;
    const elt: Elt = (t, f, to, c) => cx.elt(t, f, to, c);
    const from = cx.lineStart + line.pos;
    const children: Element[] = [cx.elt("MathMark", from, from + 2), ...tokens(first.slice(2), from + 2, elt, "math")];
    let end = cx.lineStart + line.text.length;
    while (cx.nextLine() && inside(cx, line) && line.next >= 0) {
      const text = line.text.slice(line.pos);
      const at = cx.lineStart + line.pos;
      const close = closeDisplay(text, 0);
      end = cx.lineStart + line.text.length;
      if (close < 0) {
        children.push(...tokens(text, at, elt, "math"));
        continue;
      }
      children.push(...tokens(text.slice(0, close), at, elt, "math"), cx.elt("MathMark", at + close, at + close + 2));
      end = at + close + 2;
      cx.nextLine();
      break;
    }
    cx.addElement(cx.elt("DisplayMath", from, end, children));
    return true;
  },
};

export { MATH_ENVS };
