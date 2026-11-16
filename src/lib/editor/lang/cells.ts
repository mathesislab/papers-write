// The code of a fenced block in Julia, Python or R: Quarto's cells (```` ```{julia} ````) and plain fences
// (```` ```julia ````) alike. A small stream language of its own, not the language's grammar (no package of those
// languages is needed): comments, strings, numbers and keywords, and a cell's options (`#| label: fig-x`) apart.

import { Language, StreamLanguage, type StreamParser } from "@codemirror/language";
import { tags } from "@lezer/highlight";

const KEYWORDS: Record<string, string> = {
  julia:
    "function end if elseif else for while return begin let local global const struct mutable module using import export do try catch finally break continue macro quote abstract primitive type where in isa true false nothing missing",
  python:
    "def return if elif else for while in not and or is import from as with try except finally raise class pass break continue lambda yield global nonlocal assert del async await True False None",
  r: "function if else for while repeat in next break return TRUE FALSE NULL NA NaN Inf library require",
};

/** The tokenizer of `lang` (`julia`, `python`, `r`). */
export function cellParser(lang: string): StreamParser<null> {
  const keywords = new Set((KEYWORDS[lang] ?? "").split(" "));
  // a quote in Julia is a character or a transpose (`x'`): strings only in double quotes there
  const single = lang !== "julia";
  return {
    name: lang,
    startState: () => null,
    languageData: { commentTokens: { line: "#" } },
    tokenTable: { meta: tags.meta, comment: tags.comment, string: tags.string, number: tags.number, keyword: tags.keyword },
    token(stream) {
      if (stream.sol() && stream.match(/^#\|.*/)) return "meta";
      if (stream.match(/^#.*/)) return "comment";
      if (stream.match(/^"(?:[^"\\]|\\.)*"?/) || (single && stream.match(/^'(?:[^'\\]|\\.)*'?/))) return "string";
      if (stream.match(/^\d[\d_]*(?:\.\d*)?(?:[eE][+-]?\d+)?/)) return "number";
      const word = stream.match(/^[A-Za-z_][\w!]*/) as RegExpMatchArray | null;
      if (word) return keywords.has(word[0]) ? "keyword" : null;
      stream.next();
      return null;
    },
  };
}

const languages = new Map<string, Language>();

/** The language of a fenced block's info (`{julia}`, `julia`, `{python echo=FALSE}`...), or null for any other. */
export function cellLanguage(info: string): Language | null {
  const lang = /^\{?\s*(julia|python|r)\b/i.exec(info.trim())?.[1]?.toLowerCase();
  if (!lang) return null;
  let language = languages.get(lang);
  if (!language) languages.set(lang, (language = StreamLanguage.define(cellParser(lang))));
  return language;
}
