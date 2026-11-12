//! Spell checking: the engine, results and the dictionaries box.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::input::TextareaState;
use gpui_kit::*;

pub(crate) struct SpellUi {
    /// codex:spellcheck-native: the spellcheck engine (macOS: the system
    /// NSSpellChecker; elsewhere the embedded English wordlist), shared
    /// with background check tasks.
    pub(super) checker: std::sync::Arc<quill::spellcheck::SpellChecker>,
    /// Which engine that is and its dictionaries (Appearance → Spelling).
    pub(super) info: super::spellcheck_ui::SpellInfo,
    /// Appearance → Spelling → Manage dictionaries (parity:appearance-dictionaries).
    pub(super) dict_manager: Box<super::spell_dictionaries::DictManager>,
    pub(super) dict_filter_input: Entity<TextareaState>,
    /// Builds the platform engine for the user's language picks; runs on a
    /// background task (a field so tests can slow it down).
    pub(super) engine_builder: EngineBuilder,
    pub(super) engine_build: EngineBuild,
}

/// The background engine build: holding the task is the building stage, and
/// replacing or dropping it discards the result, so only the newest build
/// ever installs.
#[derive(Default)]
pub(super) enum EngineBuild {
    #[default]
    Idle,
    Building {
        _task: Task<()>,
    },
}

impl EngineBuild {
    #[cfg(test)]
    pub(super) fn is_building(&self) -> bool {
        matches!(self, EngineBuild::Building { .. })
    }
}

pub(super) type EngineBuilder = std::sync::Arc<
    dyn Fn(
            &[String],
        ) -> (
            std::sync::Arc<quill::spellcheck::SpellChecker>,
            super::spellcheck_ui::SpellInfo,
        ) + Send
        + Sync,
>;

impl SpellUi {
    pub(super) fn new(
        spellchecker: std::sync::Arc<quill::spellcheck::SpellChecker>,
        spell_info: super::spellcheck_ui::SpellInfo,
        dict_filter_input: Entity<TextareaState>,
    ) -> Self {
        Self {
            // codex:spellcheck-native: platform engine + persisted app words.
            checker: spellchecker,
            info: spell_info,
            dict_manager: Default::default(),
            dict_filter_input,
            engine_builder: std::sync::Arc::new(super::app::QuillApp::build_spell_engine),
            engine_build: EngineBuild::Idle,
        }
    }
}

/// One composer's spelling: what was checked and what is underlined. Each
/// window's composer has its own (`ComposerUi::spelling`, swapped with
/// the window's state), so typing in one window keeps another window's
/// underlines.
#[derive(Default)]
pub(crate) struct ComposerSpelling {
    pub(super) underlines: Underlines,
    pub(super) check: Check,
}

/// The composer text last checked with the misspellings found in it: one
/// value, since the ranges only mean something against that text.
#[derive(Default)]
pub(crate) struct Underlines {
    /// Misspellings underlined in the composer; byte ranges into `text`.
    pub(super) found: Vec<quill::spellcheck::Misspelling>,
    /// The composer text `found` refers to (shifted on every edit, so
    /// underlines follow the text before the re-check lands).
    pub(super) text: String,
}

/// The debounced background re-check. Holding the task is the checking
/// stage: dropping it cancels the check.
#[derive(Default)]
pub(crate) enum Check {
    #[default]
    Idle,
    Checking {
        /// Held so that dropping the stage cancels the check.
        _task: Task<()>,
    },
}
