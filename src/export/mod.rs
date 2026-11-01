//! Exports to HTML and JSON with media, modeled on tdesktop's `export/`.
//!
//! - A chat export (`parity:data-chat-export-html`): one chat's history
//!   with a date range and size limit, as `messages.html`,
//!   `messages2.html`, ... plus media folders and `result.json`.
//! - A full account export (`parity:data-export-full`,
//!   `parity:platform-data-export`, [`account`]): the "Export Telegram
//!   data" box's sections, written in tdesktop's layout with an
//!   `export_results.html` index, `lists/` pages, `chats/chat_NN/` folders
//!   and one `result.json`.
//!
//! The work runs on a worker thread ([`worker`]) that talks to TDLib through
//! the connect driver ([`handle`]), so the UI thread only polls progress.
#![deny(clippy::indexing_slicing)]
pub mod account;
pub mod assets;
pub mod collect;
pub(crate) mod copy;
pub mod handle;
pub mod html;
pub mod html_media;
pub mod json;
pub mod leftovers;
pub mod loaded_files;
pub mod model;
pub mod names;
pub mod settings;
pub mod text;
pub mod userpic;
pub mod worker;

#[cfg(test)]
mod tests;

pub use account::{AccountExport, AccountSettings};
pub use handle::ChatExport;
pub use settings::{ExportFormat, ExportSettings, MediaKind, MediaTypes};
pub use worker::{Progress, Stage};
