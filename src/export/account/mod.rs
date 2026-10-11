//! The full account export: tdesktop's "Export Telegram data"
//! (`parity:data-export-full`, `parity:platform-data-export`).
//!
//! Layout, as tdesktop writes it: `export_results.html` (personal
//! information and links), `lists/*.html` (profile pictures, stories,
//! profile music, contacts, frequent contacts, sessions, web sessions,
//! chats), `chats/chat_NN/messages*.html` with each chat's media folders,
//! `profile_pictures/`, `stories/`, `profile_music/files/`, `css/`, `js/`,
//! and one `result.json` with every section.
pub mod dialogs;
pub mod handle;
pub mod html;
pub mod json;
pub mod sections;
pub mod settings;
pub mod worker;

#[cfg(test)]
mod tests;

pub use handle::AccountExport;
pub use settings::{AccountSettings, Section, Sections};
pub use worker::{AccountProgress, AccountStage, AccountSummary};
