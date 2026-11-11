//! The AI compose box's UI state (`ui/ai_compose/`): the open box, the
//! sub-view it shows, the language search, the style editor's fields, the
//! style preview, the apply-in-place request, the composer button's
//! fade-in and the custom emoji the style pills asked for.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::ai_compose::{AiComposeBox, StyleExample, StyleExamples, TextCompositionStyle};
use quill::ids::{AccountKey, ChatId, ConversationKey, RequestId};
use quill::lang::{keys, tr};
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
    pub(super) save: StyleSave,
}

/// Where the editor's Create or Save stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleSave {
    /// Being edited: Create or Save may be pressed.
    Editing,
    /// Create or Save was sent and has no answer yet.
    Sent,
}

/// Where the style preview stands: the lookup its link started, the style
/// it shows, and the Add, Remove or Delete sent from it.
pub(crate) enum PreviewStage {
    /// The `searchTextCompositionStyle` request with this token runs.
    LookingUp {
        token: u64,
    },
    Shown(TextCompositionStyle),
    /// Add Style was sent as `request`: the button waits, and only that
    /// request's answer closes the preview (its error shows it again).
    Adding {
        style: TextCompositionStyle,
        request: RequestId,
    },
    /// Remove or Delete was confirmed here and sent as `request`, so its
    /// toast shows and the preview closes on that request's answer
    /// (tdesktop shows those toasts only from this box); an error shows
    /// the preview again.
    Removing {
        style: TextCompositionStyle,
        request: RequestId,
    },
}

/// The style preview an `addstyle/<name>` link opens (tdesktop
/// `PreviewAiToneBox`).
pub(crate) struct AiStylePreview {
    pub(super) name: String,
    pub(super) stage: PreviewStage,
    pub(super) examples: StyleExamples,
    /// The example shown; one at a time.
    pub(super) example: Option<StyleExample>,
    pub(super) example_token: u64,
    /// When the example being loaded was asked for (the refresh icon spins
    /// from here).
    pub(super) example_asked_at: Option<Instant>,
}

impl AiStylePreview {
    /// The style, once the lookup answered.
    pub(super) fn style(&self) -> Option<&TextCompositionStyle> {
        match &self.stage {
            PreviewStage::LookingUp { .. } => None,
            PreviewStage::Shown(style)
            | PreviewStage::Adding { style, .. }
            | PreviewStage::Removing { style, .. } => Some(style),
        }
    }

    pub(super) fn into_style(self) -> Option<TextCompositionStyle> {
        match self.stage {
            PreviewStage::LookingUp { .. } => None,
            PreviewStage::Shown(style)
            | PreviewStage::Adding { style, .. }
            | PreviewStage::Removing { style, .. } => Some(style),
        }
    }

    /// The lookup's token while it runs.
    pub(super) fn lookup_token(&self) -> Option<u64> {
        match self.stage {
            PreviewStage::LookingUp { token } => Some(token),
            _ => None,
        }
    }

    /// Whether Add Style was sent and not answered.
    pub(super) fn adding(&self) -> bool {
        matches!(self.stage, PreviewStage::Adding { .. })
    }

    /// Whether `request` is the Add, Remove or Delete this preview waits on.
    pub(super) fn waits_on(&self, request: RequestId) -> bool {
        matches!(
            &self.stage,
            PreviewStage::Adding { request: asked, .. }
                | PreviewStage::Removing { request: asked, .. } if *asked == request
        )
    }

    pub(super) fn is_adding_as(&self, request: RequestId) -> bool {
        matches!(&self.stage, PreviewStage::Adding { request: asked, .. } if *asked == request)
    }

    pub(super) fn is_removing_as(&self, request: RequestId) -> bool {
        matches!(&self.stage, PreviewStage::Removing { request: asked, .. } if *asked == request)
    }

    /// The Add, Remove or Delete failed or never left: show the style again.
    pub(super) fn show_again(&mut self) {
        let stage = std::mem::replace(&mut self.stage, PreviewStage::LookingUp { token: 0 });
        self.stage = match stage {
            PreviewStage::Adding { style, .. } | PreviewStage::Removing { style, .. } => {
                PreviewStage::Shown(style)
            }
            other => other,
        };
    }
}

/// The apply-in-place shortcut's request (tdesktop `TriggerAiApplyInPlace`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApplyInPlace {
    pub(super) token: u64,
    /// The account whose style the shortcut used: a style-gone answer
    /// clears that account's binding and no other.
    pub(super) account: AccountKey,
    /// The composer's whole text at the press. The answer rewrites a
    /// range of it, so it lands only while the composer still holds it.
    pub(super) field: String,
    /// The conversation whose composer asked: its chat and topic, thread
    /// or Saved sublist. The answer lands only while the view shows it, so
    /// a rewrite asked in one forum topic never replaces the next topic's
    /// draft.
    pub(super) conversation: ConversationKey,
    /// The window whose composer asked (`None`: the main window's); the
    /// answer rewrites that composer.
    pub(super) window: Option<ChatId>,
    /// The rewritten selection; `None` for the whole field.
    pub(super) selection: Option<Range<usize>>,
    /// The caret when the request was sent (kept after a whole-field
    /// rewrite, clamped to the new text).
    pub(super) cursor: usize,
}

pub(crate) struct AiComposeOpen {
    /// The conversation whose draft the box rewrites (chat and topic,
    /// thread or Saved sublist). The box closes when its window shows
    /// another one, and Apply and Send act only while it is shown.
    pub(super) conversation: ConversationKey,
    /// The window whose composer the box was opened over (`None`: the
    /// main window's). The box closes when that window shows another
    /// chat, not when the main window does.
    pub(super) window: Option<ChatId>,
    pub(super) ai: AiComposeBox,
    pub(super) view: AiComposeView,
    /// The style names the box last refreshed against.
    pub(super) style_names: Vec<String>,
    pub(super) editor: Option<StyleEditor>,
    /// A style just created or edited, selected once the box's style list
    /// has it (tdesktop `selectToneById`).
    pub(super) select_saved: Option<String>,
}

pub(crate) struct AiComposeUi {
    pub(super) open: Option<AiComposeOpen>,
    pub(super) language_search: Entity<TextareaState>,
    pub(super) style_name: Entity<TextareaState>,
    pub(super) style_prompt: Entity<TextareaState>,
    pub(super) preview: Option<AiStylePreview>,
    pub(super) apply_in_place: Option<ApplyInPlace>,
    /// When the composer's AI button appeared (its sparkle blinks from
    /// here); `None` while it is hidden.
    pub(super) button_shown_at: Option<Instant>,
}

impl crate::ui::on_screen::Farewell for AiComposeUi {}

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
        }
    }
}
