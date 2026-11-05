//! The Word round trip: the suggestions and comments of a review as the tracked changes and comments of a .docx, and
//! a .docx read back as suggestions and comments.
//!
//! The .docx itself is pandoc's: [`export`] turns the Markdown into the Markdown pandoc's docx writer makes tracked
//! changes and comments of (`[old]{.deletion author=".." date=".."}[new]{.insertion ..}`, `[note]{.comment-start
//! id="1" ..}text[]{.comment-end id="1"}`), and the import reads the .docx by pandoc, and places what it finds in the
//! Markdown (`import`, with the search of `places` that ignores the Markdown's punctuation). `protect` finds what
//! pandoc must not be given spans inside: math, code and raw LaTeX.

pub mod export;
pub mod protect;

pub use export::{Export, markdown_with_changes};
