//! Settings → Language: the open box and the packs used before
//! (docs/decisions/language-packs.md).

use super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::prefs::LanguageChoice;

/// The Language box while it is open.
pub(crate) struct LanguageBox {
    pub(super) search: Entity<TextareaState>,
    pub(super) _search_subscription: Subscription,
    /// Recent packs deleted in this box (shown dimmed with "Restore").
    pub(super) removed: Vec<String>,
}

pub(crate) struct LanguageUi {
    pub(super) open: Option<LanguageBox>,
    /// Packs used before, newest first (`language_recent.json`).
    pub(super) recent: Vec<LanguageChoice>,
    /// The pack a `t.me/setlanguage` link asked to apply, waiting for its
    /// strings.
    pub(super) applying_link: Option<String>,
}

impl LanguageUi {
    /// Loads the recent packs and, outside screenshot demos, installs the
    /// strings the last run used so the first frame is already translated.
    pub(super) fn new(demo: bool, _cx: &mut Context<QuillApp>) -> Self {
        let root = (!demo).then(quill::settings::safe_app_root).flatten();
        let recent = root
            .as_deref()
            .map(quill::lang::prefs::load_recent)
            .unwrap_or_default();
        if let Some(root) = root.as_deref() {
            quill::lang::install(quill::lang::prefs::load_cached_snapshot(root));
        }
        Self {
            open: None,
            recent,
            applying_link: None,
        }
    }
}
