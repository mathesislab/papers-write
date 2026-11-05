// LaTeX in the text, which pandoc passes raw: commands (`\name`, `\name*`) and environments (`\begin{name}` up to
// `\end{name}`). The content of `equation`, `align` ... is highlighted as math, that of the others as LaTeX.

import type { BlockParser, Element, InlineParser } from "@lezer/markdown";
import { MATH_ENVS, inside, readRest, tokens, type Elt } from "./tokens.ts";
import { latexBrace, latexCommand, latexEnvName } from "./tags.ts";

export { latexBrace, latexCommand, latexEnvName };

const ESC = /[.*+?^${}()|[\]\\]/g;

/** `\name` and `\name*` in the text (`\begin{name}` and `\end{name}` with their braces). */
export const latexInline: InlineParser = {
  name: "LatexCommand",
  parse(cx, next, pos) {
    if (next !== 92) return -1;
    const m = /^\\(?:(?:begin|end)\{[^}\n]*\}|[A-Za-z]+\*?)/.exec(cx.slice(pos, Math.min(cx.end, pos + 200)));
    if (!m) return -1;
    for (const e of tokens(m[0], pos, (t, f, to, c) => cx.elt(t, f, to, c), "latex")) cx.addElement(e);
    return pos + m[0].length;
  },
  before: "Escape",
};

/** `\begin{name}` at the start of a line to `\end{name}`, as a block. */
export const latexBlock: BlockParser = {
  name: "LatexEnvironment",
  before: "HTMLBlock",
  parse(cx, line) {
    const first = line.text.slice(line.pos);
    const begin = /^\\begin\{([A-Za-z]+\*?)\}/.exec(first);
    if (!begin) return false;
    const name = begin[1];
    const end = new RegExp(`\\\\end\\{${name.replace(ESC, "\\$&")}\\}`);
    // closed somewhere below, or it is no environment
    if (!end.test(readRest(cx, cx.lineStart + line.pos))) return false;
    const mode = MATH_ENVS.test(name) ? "math" : "latex";
    const elt: Elt = (t, f, to, c) => cx.elt(t, f, to, c);
    const from = cx.lineStart + line.pos;
    const children: Element[] = [];
    let to = from;
    let text = first;
    for (;;) {
      const at = cx.lineStart + line.pos;
      const close = end.exec(text);
      to = at + (close ? close.index + close[0].length : text.length);
      children.push(...tokens(close ? text.slice(0, close.index + close[0].length) : text, at, elt, mode));
      if (close) {
        cx.nextLine();
        break;
      }
      if (!cx.nextLine() || !inside(cx, line)) break;
      text = line.text.slice(line.pos);
    }
    cx.addElement(cx.elt(mode === "math" ? "MathEnvironment" : "LatexEnvironment", from, to, children));
    return true;
  },
};
