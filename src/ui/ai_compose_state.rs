//! The AI compose box's UI state (`ui/ai_compose/`): the open box, the
//! sub-view it shows, the language search, the style editor's fields, the
//! style preview, the apply-in-place request, the composer button's
//! fade-in and the custom emoji the style pills asked for.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::ai_compose::{AiComposeBox, StyleExample, StyleExamples, TextCompositionStyle};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use std::collections::HashSet;
use std::ops::Range;
use std::time::Instant;

/// What the box shows. tdesktop opens the language picker, the Cocoon
/// box, the style editor and its icon picker as boxes of their own;
/// Quill's dialogs never stack, so they are views inside the one dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiComposeView {
    Card,
    Languages,
    About,
    StyleEditor,
    StyleIcon,
}

/// The editor field a failed check points at (tdesktop's
/// `WarningTooltip` next to it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorWarning {
    Icon,
    Name,
    Prompt,
}

/// The New Style / Edit Style view (tdesktop `CreateAiToneBox`,
/// `EditAiToneBox`); the name and prompt live in `AiComposeUi`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StyleEditor {
    /// The edited style's name; `None` for a new style.
    pub(super) editing: Option<String>,
    /// 0 until an icon is picked.
    pub(super) emoji_id: i64,
    /// "Add a link to my account".
    pub(super) show_creator: bool,
    pub(super) warning: Option<EditorWarning>,
    /// Create or Save was sent and has no answer yet.
    pub(super) pending: bool,
}

/// The style preview an `addstyle/<name>` link opens (tdesktop
/// `PreviewAiToneBox`).
pub(crate) struct AiStylePreview {
    pub(super) name: String,
    /// The `searchTextCompositionStyle` request's token.
    pub(super) token: u64,
    /// `None` while the lookup runs.
    pub(super) style: Option<TextCompositionStyle>,
    pub(super) examples: StyleExamples,
    /// The example shown; one at a time.
    pub(super) example: Option<StyleExample>,
    pub(super) example_token: u64,
    /// When the example being loaded was asked for (the refresh icon spins
    /// from here).
    pub(super) example_asked_at: Option<Instant>,
    /// Remove or Delete was confirmed here, so its toast shows and the
    /// preview closes (tdesktop shows those toasts only from this box).
    pub(super) removing: bool,
}

/// The apply-in-place shortcut's request (tdesktop `TriggerAiApplyInPlace`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApplyInPlace {
    pub(super) token: u64,
    pub(super) chat_id: ChatId,
    /// The rewritten selection; `None` for the whole field.
    pub(super) selection: Option<Range<usize>>,
    /// The caret when the request was sent (kept after a whole-field
    /// rewrite, clamped to the new text).
    pub(super) cursor: usize,
}

/// The open box and the chat whose composer it rewrites.
pub(crate) struct AiComposeOpen {
    pub(super) chat_id: ChatId,
    pub(super) ai: AiComposeBox,
    pub(super) view: AiComposeView,
    /// The style names the box last refreshed against.
    pub(super) style_names: Vec<String>,
    /// The New / Edit Style view's state while it is open.
    pub(super) editor: Option<StyleEditor>,
    /// A style just created or edited, selected once the box's style list
    /// has it (tdesktop `selectToneById`).
    pub(super) select_saved: Option<String>,
}

pub(crate) struct AiComposeUi {
    pub(super) open: Option<AiComposeOpen>,
    /// The language picker's search field.
    pub(super) language_search: Entity<TextareaState>,
    /// The style editor's name and prompt fields.
    pub(super) style_name: Entity<TextareaState>,
    pub(super) style_prompt: Entity<TextareaState>,
    pub(super) preview: Option<AiStylePreview>,
    pub(super) apply_in_place: Option<ApplyInPlace>,
    /// When the composer's AI button appeared (its sparkle blinks from
    /// here); `None` while it is hidden.
    pub(super) button_shown_at: Option<Instant>,
    /// Custom emoji ids the style pills already asked TDLib for.
    pub(super) emoji_asked: HashSet<i64>,
}

impl AiComposeUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let language_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_LANGUAGES))
                .auto_grow(1, 1)
        });
        let style_name = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::AI_COMPOSE_TONE_NAME_PLACEHOLDER))
                .auto_grow(1, 1)
        });
        let style_prompt = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::AI_COMPOSE_TONE_PROMPT_PLACEHOLDER))
                .auto_grow(4, 10)
        });
        cx.subscribe_in(
            &language_search,
            window,
            |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        )
        .detach();
        for (field, single_line) in [(&style_name, true), (&style_prompt, false)] {
            cx.subscribe_in(
                field,
                window,
                move |this, field, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.clamp_ai_style_field(field, single_line, window, cx);
                        cx.notify();
                    }
                },
            )
            .detach();
        }
        Self {
            open: None,
            language_search,
            style_name,
            style_prompt,
            preview: None,
            apply_in_place: None,
            button_shown_at: None,
            emoji_asked: HashSet::new(),
        }
    }
}
