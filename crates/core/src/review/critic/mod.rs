//! The CriticMarkup round trip (criticmarkup.com), for a reader or a co-author without the app (or Word): the document
//! with the open suggestions and comments as marks in the Markdown ([`export`]), and a file of such marks read back
//! as suggestions and comments ([`import`]). `{++new++}`, `{--old--}`, `{~~old~>new~~}`, `{==text==}{>>note<<}`.

pub mod export;
pub mod import;

pub use export::{Export, markdown_with_changes};
pub use import::{CRITIC, import};
