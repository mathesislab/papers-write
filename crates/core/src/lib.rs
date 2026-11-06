//! The references of a paper written in Markdown, and the bibliography that grows as its keys are cited.
//!
//! - [`frontmatter`]: the `bibliography` field of a Markdown text's front matter, read and added.
//! - [`refs`]: the `.bib` files of a document and the papers library, loaded as one library and searched.
//! - [`latex`]: the same for a LaTeX file: `\bibliography`, `\addbibresource`, the keys cited.
//! - [`srcmap`]: the line of the Markdown that a line of its texmark LaTeX came from (matched by words).
//! - [`bib`]: a cited key of the library copied into the document's `.bib`; the keys cited and missing or unused.
//! - [`review`]: suggestions and comments kept in files beside the document, carried onto its current text.
//! - [`exchange`]: the Word side of a review (pandoc): the .docx with tracked changes out, a .docx read back in.
//! - [`colour`]: the colours of a project, by name (papers-review's).
//! - [`project`]: a folder with `write.toml`, and the project a document belongs to.
//! - [`files`]: the files of a project as a tree (papers-review's).
//! - [`search`]: a project searched, and text replaced in its files.

pub mod bib;
pub mod colour;
pub mod exchange;
pub mod files;
pub mod frontmatter;
pub mod latex;
pub mod project;
pub mod refs;
pub mod review;
pub mod search;
pub mod srcmap;

pub use bib::{Cited, Error, cite, cited_keys, missing, unused};
pub use refs::{Found, RefEntry, References, bibliography_of, project_library_of, sources_in, sources_of};
