// The types of the project bar.

/** The colours of a project, by name (mirrors write_core::colour::Colour); none: no colour. */
export type Colour = "blue" | "teal" | "green" | "yellow" | "orange" | "red" | "pink" | "purple" | "graphite" | "none";

/** What a file is, for how it shows (mirrors write_core::files::Kind). */
export type Kind = "folder" | "pdf" | "markdown" | "latex" | "text" | "bib" | "image" | "other";

/** A file or folder of a project's tree (mirrors write_core::files::Node). */
export type Node = { name: string; path: string; kind: Kind; children?: Node[]; missing?: boolean };
