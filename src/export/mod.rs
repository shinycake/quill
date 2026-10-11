//! Chat export to HTML and JSON with media, a date range and a size limit
//! (`parity:data-chat-export-html`), modeled on tdesktop's single-chat
//! export (`export/`): the same media kinds and size steps, the folder
//! layout (`messages.html`, `messages2.html`, `photos/`, `files/`, ...),
//! 1000 messages per HTML file, and `result.json`.
//!
//! The work runs on a worker thread ([`worker`]) that talks to TDLib through
//! the connect driver ([`handle`]), so the UI thread only polls progress.
pub mod handle;
pub mod html;
pub mod json;
pub mod model;
pub mod names;
pub mod settings;
pub mod text;
pub mod worker;

#[cfg(test)]
mod tests;

pub use handle::ChatExport;
pub use settings::{ExportFormat, ExportSettings, MediaKind, MediaTypes};
pub use worker::{Progress, Stage};
