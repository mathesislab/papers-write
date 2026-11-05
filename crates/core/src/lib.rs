//! The references of a paper written in Markdown, and the bibliography that grows as its keys are cited.
//!
//! - [`frontmatter`]: the `bibliography` field of a Markdown text's front matter, read and added.
//! - [`refs`]: the `.bib` files of a document and the papers library, loaded as one library and searched.
//! - [`latex`]: the same for a LaTeX file: `\bibliography`, `\addbibresource`, the keys cited.
//! - [`bib`]: a cited key of the library copied into the document's `.bib`; the keys cited and missing or unused.
//! - [`review`]: suggestions and comments kept in files beside the document, carried onto its current text.
//! - [`colour`]: the colours of a project, by name (papers-review's).
//! - [`project`]: a folder with `write.toml`, and the project a document belongs to.

pub mod bib;
pub mod colour;
pub mod frontmatter;
pub mod latex;
pub mod project;
pub mod refs;
pub mod review;

pub use bib::{Cited, Error, cite, cited_keys, missing, unused};
pub use refs::{Found, RefEntry, References, bibliography_of, sources_of};
