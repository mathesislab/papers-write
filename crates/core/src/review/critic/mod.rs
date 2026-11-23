//! The CriticMarkup round trip (criticmarkup.com), for a reader or a co-author without the app (or Word): the document
//! with the open suggestions and comments as marks in the Markdown ([`export`]), and a file of such marks read back
//! as suggestions and comments ([`import`]). `{++new++}`, `{--old--}`, `{~~old~>new~~}`, `{==text==}{>>note<<}`.
//! The marks as the preview shows them, or the text with them all accepted or rejected: [`view`].

pub mod export;
pub mod import;
pub mod view;

pub use export::{Export, in_text, markdown_with_changes};
pub use import::{CRITIC, import};
