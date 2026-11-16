// The code of a fenced block in Julia, Python or R: Quarto's cells (```` ```{julia} ````) and plain fences
// (```` ```julia ````) alike, coloured by CodeMirror's modes of those languages (@codemirror/legacy-modes), with a
// cell's options (`#| label: fig-x`) apart.

import { Language, StreamLanguage, type StreamParser } from "@codemirror/language";
import { julia } from "@codemirror/legacy-modes/mode/julia";
import { python } from "@codemirror/legacy-modes/mode/python";
import { r } from "@codemirror/legacy-modes/mode/r";

const MODES: Record<string, StreamParser<unknown>> = { julia, python, r };

/** `mode` with a cell's option lines (`#| label: fig-x`, at the start of a line) as `meta`, not comments. */
export function withCellOptions<S>(mode: StreamParser<S>): StreamParser<S> {
  return {
    ...mode,
    token(stream, state) {
      if (stream.sol() && stream.match(/^#\|.*/)) return "meta";
      return mode.token(stream, state);
    },
  };
}

/** The tokenizer of `lang` (`julia`, `python`, `r`), a cell's options apart. */
export const cellParser = (lang: string): StreamParser<unknown> => withCellOptions(MODES[lang]);

const languages = new Map<string, Language>();

/** The language of a fenced block's info (`{julia}`, `julia`, `{python echo=FALSE}`...), or null for any other. */
export function cellLanguage(info: string): Language | null {
  const lang = /^\{?\s*(julia|python|r)\b/i.exec(info.trim())?.[1]?.toLowerCase();
  if (!lang) return null;
  let language = languages.get(lang);
  if (!language) languages.set(lang, (language = StreamLanguage.define(cellParser(lang))));
  return language;
}
