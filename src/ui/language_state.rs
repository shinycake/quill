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
    /// Recent packs deleted in this box, with the place each had, shown
    /// dimmed with "Restore". They are already out of `LanguageUi::recent`,
    /// so every save writes the one list.
    pub(super) removed: Vec<(usize, LanguageChoice)>,
}

/// The files the Language box writes, each kept in the order of the
/// writes that produced it.
#[derive(Clone, Copy)]
pub(super) enum LanguageFile {
    Choice = 0,
    Recent = 1,
    Cache = 2,
}

/// Background writes of the language files. Each file's writes run one
/// after the other in the order they were asked (a fast A → B switch never
/// leaves A on disk): a write waits for the task of the one before it, which
/// it owns.
#[derive(Default)]
pub(super) struct LanguageWrites {
    files: [WriteStage; 3],
}

/// The last write asked of one file: none yet, or the task that waits for
/// its predecessor and then writes.
#[derive(Default)]
enum WriteStage {
    #[default]
    Idle,
    Writing {
        task: Task<()>,
    },
}

impl LanguageWrites {
    pub(super) fn spawn(
        &mut self,
        file: LanguageFile,
        cx: &mut Context<QuillApp>,
        write: impl FnOnce() + Send + 'static,
    ) {
        let Some(stage) = self.files.get_mut(file as usize) else {
            return;
        };
        let previous = std::mem::take(stage);
        let task = cx.background_spawn(async move {
            if let WriteStage::Writing { task } = previous {
                task.await;
            }
            write();
        });
        *stage = WriteStage::Writing { task };
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
            super::language_ui::install_pack(quill::lang::prefs::load_cached_snapshot(root));
        }
        Self {
            open: None,
            recent,
            writes: LanguageWrites::default(),
        }
    }
}
