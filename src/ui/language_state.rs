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
pub(super) struct LanguageWrites {
    files: [WriteStage; 3],
    /// Quitting waits for the writes still out, so a quick A → B switch
    /// and quit leaves B on disk, not nothing or A.
    _flush_on_quit: Subscription,
}

/// The last write asked of one file: none yet, or the task of the newest
/// write, which waits for its predecessor and then writes. The task stays
/// here after its write lands (a finished task awaits at once), so the next
/// write can wait on it and quitting can wait on it.
#[derive(Default)]
enum WriteStage {
    #[default]
    Idle,
    Last {
        task: Task<()>,
    },
}

impl LanguageWrites {
    pub(super) fn new(cx: &mut Context<QuillApp>) -> Self {
        let flush = cx.on_app_quit(|app: &mut QuillApp, _| {
            let pending = app.language.writes.take_pending();
            async move {
                for task in pending {
                    task.await;
                }
            }
        });
        Self {
            files: Default::default(),
            _flush_on_quit: flush,
        }
    }

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
            if let WriteStage::Last { task } = previous {
                task.await;
            }
            write();
        });
        *stage = WriteStage::Last { task };
    }

    /// Every file's newest write, taken out to be awaited: a write chain
    /// ends with its newest task, so awaiting these awaits them all.
    pub(super) fn take_pending(&mut self) -> Vec<Task<()>> {
        self.files
            .iter_mut()
            .filter_map(|stage| match std::mem::take(stage) {
                WriteStage::Last { task } => Some(task),
                WriteStage::Idle => None,
            })
            .collect()
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
    pub(super) fn new(demo: bool, cx: &mut Context<QuillApp>) -> Self {
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
            writes: LanguageWrites::new(cx),
        }
    }
}
