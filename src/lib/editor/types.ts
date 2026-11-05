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
