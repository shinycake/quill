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
    pub(super) dict_manager: super::spell_dictionaries::DictManager,
    pub(super) dict_filter_input: Entity<TextareaState>,
    /// Builds the platform engine for the user's language picks; runs on a
    /// background task (a field so tests can slow it down).
    pub(super) engine_builder: EngineBuilder,
    /// The pending background engine build (dropping it discards the result).
    pub(super) engine_task: Option<Task<()>>,
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
            engine_task: None,
        }
    }
}

/// One composer's spelling: what was checked and what is underlined. Each
/// window's composer has its own (`ComposerUi::spelling`, swapped with
/// the window's state), so typing in one window keeps another window's
/// underlines.
#[derive(Default)]
pub(crate) struct ComposerSpelling {
    /// Misspellings underlined in the composer; byte ranges into
    /// `checked_text`.
    pub(super) misspellings: Vec<quill::spellcheck::Misspelling>,
    /// The composer text `misspellings` refers to (shifted on every edit,
    /// so underlines follow the text before the re-check lands).
    pub(super) checked_text: String,
    /// The debounced background re-check (dropping it cancels it).
    pub(super) task: Option<Task<()>>,
}
