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
export { notProse, spelling } from "./spelling";
export { currentHeading, headingTitle, outline, type Heading } from "./outline";
export { sections, type Section } from "./sections";
export { DEFAULTS, LATEX, find as findSymbols, latex, merged, typed, type Entry, type Rule } from "./symbols";
export { completionSource } from "./completion";
export { citationKeys, diagnostics, labelProblems, recheck, type Problem } from "./diagnostics";
export { environmentTex, hover, isTex, labelTarget, targetAt, type Target } from "./hover";
export { contextAt, labelsOf, literalAt, nameOptions, refOptions, texContextAt, texLabelsOf, type Context, type Literal } from "./context";
export { MATH_NAMES, TEXT_NAMES, TEX_NAMES, type LatexName } from "./lang/latex-names.ts";
export type { CitationOption, Completion, Diagnostics, EditorFont, Found, Hit, HoverSources, Indent, Item, Span, YearCount } from "./types";
export { accept, itemsAt, redoSuggestion, review, reviewColour, REVIEW_COLOURS, setOwnColour, reviewMode, reviewView, scrollToItem, setReview, setReviewMode, setReviewView, undoSuggestion } from "./review/review";
export type {
  ReviewDecision,
  ReviewHandlers,
  ReviewItem,
  ReviewKind,
  ReviewMode,
  ReviewView,
  ReviewRecord,
  ReviewState,
  ReviewStatus,
  Suggested,
} from "./review/types";
export { default as FileEditor } from "./FileEditor.svelte";
export type { Status } from "./FileEditor.svelte";
export { default as Outline } from "./Outline.svelte";
export { default as RefList } from "./RefList.svelte";
export { default as SymbolList } from "./SymbolList.svelte";
