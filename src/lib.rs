//! ethang.dev as a fully static site, rendered by Rust.
//!
//! Every module here exists to turn `content/` into `dist/`: plain HTML, CSS,
//! and assets. No JavaScript is produced at any point, and `tests/no_javascript.rs`
//! enforces that over both the repository and the build output.

pub mod content;
pub mod highlight;
pub mod html;
pub mod images;
pub mod markdown;
pub mod math;
pub mod pages;
