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

/// The files the Language box writes, each kept in the order of the
/// writes that produced it.
#[derive(Clone, Copy)]
pub(super) enum LanguageFile {
    Choice = 0,
    Recent = 1,
    Cache = 2,
}

/// Background writes of the language files: a write only lands when no
/// newer write of the same file has (a fast A → B switch never leaves A on
/// disk).
#[derive(Default)]
pub(super) struct LanguageWrites {
    issued: [u64; 3],
    written: std::sync::Arc<std::sync::Mutex<[u64; 3]>>,
}

impl LanguageWrites {
    pub(super) fn spawn(
        &mut self,
        file: LanguageFile,
        cx: &mut Context<QuillApp>,
        write: impl FnOnce() + Send + 'static,
    ) {
        let slot = file as usize;
        let Some(issued) = self.issued.get_mut(slot) else {
            return;
        };
        *issued = issued.wrapping_add(1);
        let seq = *issued;
        let written = self.written.clone();
        cx.background_spawn(async move {
            let mut written = written
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(last) = written.get_mut(slot)
                && seq > *last
            {
                write();
                *last = seq;
            }
        })
        .detach();
    }
}

pub(crate) struct LanguageUi {
    pub(super) open: Option<LanguageBox>,
    pub(super) writes: LanguageWrites,
    /// Packs used before, newest first (`language_recent.json`).
    pub(super) recent: Vec<LanguageChoice>,
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
            writes: LanguageWrites::default(),
        }
    }
}
