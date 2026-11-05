// The types the editor and its components take from the app. The references mirror papers_core's (the app's Rust),
// the settings those of the editor's part of the app's settings.

/** mirrors settings::Indent */
export type Indent = "two" | "four" | "tab";

/** mirrors settings::EditorFont */
export type EditorFont = "system" | "serif" | "mono";

/** mirrors papers_core::Item: a reference of a .bib file, as the references panel shows it */
export type Item = {
  /** the key, unless another .bib file searched has the same key (then `key#2`) */
  id: string;
  key: string;
  /** the BibTeX entry type, e.g. "article" */
  kind: string;
  title: string;
  /** "Given Family" */
  authors: string[];
  year: number | null;
  /** the journal or book */
  container: string | null;
  /** a preprint's server, where its journal would be */
  server: string | null;
  doi: string | null;
  url: string | null;
  keywords: string[];
  /** the PDFs of the entry */
  files: string[];
  /** the .bib file of the entry */
  bib: string;
  /** one line under the title: "Feldmann & Levermann · 2017 · Cryosphere" */
  subtitle: string;
};

/** mirrors papers_core::Span: a piece of a text shown, marked where a search matched */
export type Span = { text: string; mark: boolean };

/** mirrors papers_core::Hit */
export type Hit = {
  item: Item;
  title: Span[];
  subtitle: Span[];
  /** for each author, whether it matched */
  authors: boolean[];
};

/** mirrors papers_core::YearCount */
export type YearCount = { year: number; count: number; selected: boolean };

/** mirrors papers_core::Found: what `references` and `closest_references` return */
export type Found = {
  /** the first `limit` matches, newest first (the closest first for `closest_references`) */
  hits: Hit[];
  /** the matches there are */
  total: number;
  /** the matches of each year, the years between included (none for `closest_references`) */
  years: YearCount[];
};

/** A paper offered for a citation: its key, what shows it ("Schoof 2007"), its title, and whether the key is in the
 *  bibliography of the project yet (if not, accepting it adds it). */
export type CitationOption = { key: string; label: string; detail?: string; inBib: boolean };

/** What the app supplies for the completion of the editor (see `Options.completion`). */
export type Completion = {
  /** the papers whose key or name begins with `prefix`: from the bibliography of the document and the library */
  citations?: (prefix: string) => Promise<CitationOption[]>;
  /** a citation key was accepted: the app adds it to the bibliography */
  oncite?: (key: string) => void;
  /** the figure and other files whose path begins with `prefix` */
  paths?: (prefix: string) => Promise<string[]>;
};

/** What the app supplies for the tooltips of the editor (see `Options.hover`); each source is optional, and the target of
 *  a cross-reference needs none: the editor reads it from the text. */
export type HoverSources = {
  /** the formula `tex` rendered, as HTML with a `<math>` element (MathML), displayed (`$$…$$`) or inline */
  math?: (tex: string, display: boolean) => Promise<string>;
  /** the reference of a citation key: what names it ("Schoof (2007)") and the rest (title, journal); null if unknown */
  citation?: (key: string) => Promise<{ label: string; detail?: string } | null>;
  /** the image at a path of the document (as `![…](path)` and `\includegraphics{path}` write it) as a data URL; null if none */
  image?: (path: string) => Promise<string | null>;
};

/** What the app supplies for the diagnostics of the editor (see `Options.diagnostics`). The cross-references to labels the
 *  document lacks and the labels it defines twice are found by the editor itself. */
export type Diagnostics = {
  /** of these citation keys (each once), those found neither in the document's bibliography nor in the library; asked
   *  800 ms after the typing stops and again on `recheck` */
  missingKeys?: (keys: string[]) => Promise<Set<string>>;
};

/** What the app supplies for the spelling of the editor (see `Options.spelling`): the system's spell checker. Languages are
 *  the document's (`lang:` of its front matter, e.g. `en-GB`); none is the system's. */
export type Spelling = {
  /** the misspelled words of each text (the prose ranges of the visible lines), as offsets in the text (UTF-16) */
  check: (texts: { id: number; text: string }[], language?: string) => Promise<{ id: number; misses: { from: number; to: number }[] }[]>;
  /** what the checker suggests for a misspelled word */
  guesses: (word: string, language?: string) => Promise<string[]>;
  /** the word added to the user's dictionary */
  learn: (word: string) => void | Promise<void>;
  /** the word left alone in this document */
  ignore: (word: string) => void | Promise<void>;
};
