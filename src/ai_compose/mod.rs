//! Telegram Premium "AI compose": the text composition styles, the
//! Translate / Style / Fix box, the Fix diff, the saved-style limit and the
//! local preferences. Pure logic, no GPUI and no TDLib I/O.
//!
//! tdesktop reference: `boxes/compose_ai_box.cpp`,
//! `data/data_ai_compose_tones.cpp`, `api/api_compose_with_ai.cpp`,
//! `boxes/create_ai_tone_box.cpp`, `boxes/preview_ai_tone_box.cpp`,
//! `ui/controls/compose_ai_button_factory.cpp`. TDLib 1.8.69 exposes the
//! feature as `textCompositionStyle` (`schema/td_api.tl:181`),
//! `composeTextWithAi` (`:13094`) and `fixTextWithAi` (`:13112`).
#![deny(clippy::indexing_slicing)]

mod box_state;
mod diff;
mod limit;
pub mod prefs;
mod style;

pub use box_state::*;
pub use diff::*;
pub use limit::*;
pub use style::*;

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

/// A token no other AI compose request of this process carries, so an
/// answer for a closed box (or an older request of the same box) is never
/// taken for the current one.
pub fn fresh_token() -> u64 {
    NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
}
