//! Emoji set style (parity:emoji-set-style), after Telegram Desktop's
//! Settings → Chat → "Choose emoji set" (`chat_helpers/
//! emoji_sets_manager.cpp`).
//!
//! tdesktop draws every emoji from sprite sheets and lets the user swap
//! the built-in art for "Android", "Twemoji" or "JoyPixels" packs it
//! downloads from Telegram's servers. Quill draws emoji with the system
//! font by default and offers sprite packs it builds itself from openly
//! licensed art (Noto Emoji, Twemoji; `quill-tools/emoji-sets`), attached
//! to a GitHub release. JoyPixels is not free to redistribute and is left
//! out. See docs/decisions/emoji-set-style.md.
//!
//! - [`catalog`]: the sets, their sizes and hashes, the saved choice and
//!   each row's state line.
//! - [`pack`]: the pack format (`config.json` + WebP sheets) and unpacking.
//! - [`split`]: cutting text into plain stretches and pack emoji.
//! - [`download`]: fetching and installing a pack.
#![deny(clippy::indexing_slicing)]

pub mod catalog;
pub mod download;
pub mod font_probe;
pub mod pack;
pub mod split;
