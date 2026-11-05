// The math of a LaTeX text found without a syntax tree (the LaTeX language is a stream parser, with no math nodes):
// `$…$`, `$$…$$`, `\(…\)`, `\[…\]` and the math environments (`equation`, `align` …, starred too). An escaped `\$` is
// no delimiter and a `%` comment holds none. A pure function of the text.

import { MATH_ENVS } from "./tokens.ts";

/** A piece of math: where it is (delimiters included), its formula (without the delimiters; an environment whole,
 *  `\begin` to `\end`) and whether it is displayed. */
export type TexMath = { from: number; to: number; tex: string; display: boolean };

/** The end of the comment that begins at `i` (the line break, not included). */
const lineEnd = (text: string, i: number) => {
  const nl = text.indexOf("\n", i);
  return nl < 0 ? text.length : nl;
};

/** Where `close` ends, searching from `i`: skipping escapes and comments; -1 when not found (for inline `$`, a blank
 *  line ends the paragraph and so the search). */
function closing(text: string, i: number, close: string, paragraph: boolean): number {
  while (i < text.length) {
    const c = text[i];
    if (c === "\\") {
      if (text.startsWith(close, i)) return i + close.length;
      i += 2;
    } else if (c === "%") i = lineEnd(text, i);
    else if (text.startsWith(close, i)) return i + close.length;
    else if (paragraph && c === "\n" && /^\n[ \t]*\n/.test(text.slice(i, i + 64))) return -1;
    else i++;
  }
  return -1;
}

/** Every piece of math of `text`, in order. */
export function latexMath(text: string): TexMath[] {
  const found: TexMath[] = [];
  const add = (from: number, to: number, open: number, shut: number, display: boolean) =>
    found.push({ from, to, tex: text.slice(from + open, to - shut).trim(), display });
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === "%") i = lineEnd(text, i);
    else if (c === "\\") {
      const next = text[i + 1];
      if (next === "(" || next === "[") {
        const close = next === "(" ? "\\)" : "\\]";
        const to = closing(text, i + 2, close, false);
        if (to < 0) i += 2;
        else add(i, to, 2, 2, next === "["), (i = to);
        continue;
      }
      const env = /^\\begin\{([^}]*)\}/.exec(text.slice(i, i + 40));
      if (env && MATH_ENVS.test(env[1])) {
        const end = `\\end{${env[1]}}`;
        const to = closing(text, i + env[0].length, end, false);
        if (to > 0) {
          found.push({ from: i, to, tex: text.slice(i, to), display: true });
          i = to;
          continue;
        }
      }
      i += 2;
    } else if (c === "$") {
      const display = text[i + 1] === "$";
      const open = display ? 2 : 1;
      const to = closing(text, i + open, display ? "$$" : "$", !display);
      if (to < 0) i += open;
      else add(i, to, open, open, display), (i = to);
    } else i++;
  }
  return found;
}

/** The math `pos` is in (or at the edge of), or null. */
export function latexMathAt(text: string, pos: number): TexMath | null {
  for (const m of latexMath(text)) {
    if (m.from > pos) break;
    if (pos <= m.to) return m;
  }
  return null;
}
