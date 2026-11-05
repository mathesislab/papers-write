// The public API of the editor: what an app imports as `papers-write/editor`.

export {
  PREFS,
  inCode,
  literalIn,
  insertQuote,
  insertSymbol,
  makeEditor,
  prefsFrom,
  replaceText,
  selectedWords,
  setEditorFont,
  setPrefs,
  setRules,
  showSection,
  wordCount,
  wordsOf,
  type EditorPrefs,
  type EditorSettings,
  type Options,
} from "./editor";
export * from "./lang/index.ts";
export { difference } from "./difference";
export { citation, familyName, fullReference, initials, latex as latexCite, pandoc, splitName } from "./refs";
export { sections, type Section } from "./sections";
export { DEFAULTS, LATEX, find as findSymbols, latex, merged, typed, type Entry, type Rule } from "./symbols";
export { completionSource } from "./completion";
export { contextAt, labelsOf, literalAt, nameOptions, refOptions, type Context, type Literal } from "./context";
export { MATH_NAMES, TEXT_NAMES, type LatexName } from "./lang/latex-names.ts";
export type { CitationOption, Completion, EditorFont, Found, Hit, Indent, Item, Span, YearCount } from "./types";
export { default as FileEditor } from "./FileEditor.svelte";
export type { Status } from "./FileEditor.svelte";
export { default as RefList } from "./RefList.svelte";
export { default as SymbolList } from "./SymbolList.svelte";
