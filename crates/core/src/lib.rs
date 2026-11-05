//! The references of a paper written in Markdown, and the bibliography that grows as its keys are cited.
//!
//! - [`frontmatter`]: the `bibliography` field of a Markdown text's front matter, read and added.
//! - [`refs`]: the `.bib` files of a document and the papers library, loaded as one library and searched.

pub mod frontmatter;
pub mod refs;

pub use refs::{Found, RefEntry, References, sources_of};
