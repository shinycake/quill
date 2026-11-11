//! Composer state: menus, dialogs, reply and edit drafts. The next send's
//! attachments and options are per chat, in `composer_sends.rs`.
#![deny(clippy::indexing_slicing)]

use quill::composer::DraftTopic;
use quill::ids::ChatId;
use quill::telegram::envelope::ChatDraft;

/// A draft to clear once a send is acknowledged. `sent` is the composer
/// markup that send took: when set, the stored draft goes only while it
/// still equals it, so text kept or typed since survives a late ack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftClear {
    pub(super) chat_id: ChatId,
    pub(super) sent: Option<String>,
}

impl DraftClear {
    pub(super) fn whole_composer(chat_id: ChatId) -> Self {
        Self {
            chat_id,
            sent: None,
        }
    }

    pub(super) fn of_text(chat_id: ChatId, sent: &str) -> Self {
        Self {
            chat_id,
            sent: Some(sent.to_string()),
        }
    }

    /// Whether the ack for `chat_id` may clear its draft, given the
    /// draft text stored now.
    pub(super) fn matches(&self, chat_id: ChatId, stored: Option<&str>) -> bool {
        self.chat_id == chat_id
            && self
                .sent
                .as_deref()
                .is_none_or(|sent| stored.is_none_or(|stored| stored == sent))
    }
}

pub(crate) struct ComposerUi {
    /// Highlighted row of the composer's `@` suggestions.
    pub(super) mention_selected: usize,
    /// Composer `#hashtag` / `:emoji` autocomplete popup.
    pub(super) suggest: super::composer_suggest::SuggestUi,
    pub(super) emoji_words: super::emoji_words_ui::EmojiWordsUi,
    /// Debounces the `getInlineQueryResults` dispatch (100ms quiet
    /// window, TGX); the dropdown itself is the chat's (`ChatUi`).
    pub(super) inline_query_token: u64,
    /// Scroll position of the inline results panel (`inline_grid.rs`).
    pub(super) inline_scroll: gpui_kit::ScrollHandle,
    /// The chat an inline results button left, to come back to
    /// (`inline_return.rs`).
    pub(super) inline_return: Option<super::inline_return::InlineReturn>,
    /// MED4b: the `getLinkPreview` prefetch waiting for its URL to settle:
    /// the URL and its 500 ms timer. Armed once per new URL; a frame with
    /// the same URL leaves it running (round 18 B #7: re-arming every frame
    /// put the request off for as long as the app redrew). Schema: "Do not
    /// call this function too often"; TGX rate-limits 400ms.
    pub(super) preview_timer: Option<(String, gpui_kit::Task<()>)>,
    /// The composer text before the last keystroke, for "Replace emoji
    /// automatically" (it only reacts to one typed character).
    pub(super) prev_text: gpui_kit::SharedString,
    /// A typed markdown replacement Backspace can still take back.
    pub(super) markdown_revert: Option<super::composer_field::MarkdownRevert>,
    /// "Silent send" shortcut: the next send alone goes without sound.
    pub(super) send_silent_once: bool,
    /// "Send with Effect": the next send alone carries this effect
    /// (`messageSendOptions.effect_id`); zero for none.
    pub(super) send_effect_once: i64,
    /// The send button's options menu and its effect picker
    /// (`send_effects`).
    pub(super) send_menu: super::send_effects::SendMenuUi,
    /// A rich draft's discard and Premium boxes and the rich paste notice
    /// (`rich_draft`).
    pub(super) rich_draft: super::rich_draft::RichDraftUi,
    /// A topic or thread switch asked for the new slot's draft to be put
    /// in the composer on the next frame (`restore_draft_if_pending`).
    pub(super) draft_restore_pending: bool,
    /// The session's remote draft revision this composer last checked
    /// (`QuillApp::adopt_remote_draft`).
    pub(super) remote_draft_seen: u64,
    /// The slot this composer last showed and its stored draft then. Only
    /// a change to that draft replaces the field: text a link put in the
    /// composer survives drafts arriving for other chats.
    pub(super) shown_draft: Option<(ChatId, DraftTopic, Option<ChatDraft>)>,
    /// What the compose field's placeholder was last built from
    /// (`sync_composer_placeholder`): the facts and the language.
    pub(super) placeholder_shown: Option<(quill::composer::PlaceholderFacts, usize)>,
    /// Cross-fade timeline of the round Send / Record / Save button.
    pub(super) send_morph: std::cell::Cell<Option<quill::send_button::SendMorph>>,
    /// Phase S2: the inline-bot warning has been confirmed once this
    /// session (TGX `TUTORIAL_INLINE_SEARCH_SECRECY`, which persists;
    /// Quill keeps it per session — documented divergence).
    pub(super) inline_bot_alert_shown: bool,
    /// Files being dragged over the conversation, and what they hold
    /// (`drop_zones`).
    pub(super) drop_paths: Vec<std::path::PathBuf>,
    pub(super) drop_state: Option<quill::drop_modes::DragState>,
    /// A demo's fixed drop-zone state (no real drag in a screenshot).
    pub(super) drop_preview: Option<quill::drop_modes::DragState>,
    /// Phase A1: the open chat whose slow-mode countdown is ticking
    /// (`Some` exactly while the 1s tick task runs). Mirrors `voice_tick`.
    pub(super) slow_mode_tick_chat: Option<ChatId>,
    /// The attachment tray's file checks, price box and video editor.
    pub(super) send_box: super::send_box_state::SendBoxUi,
    pub(super) text_limit: super::composer_ui::TextLimitMemo,
    /// This composer's spell check and underlines (`spellcheck_ui`).
    pub(super) spelling: super::spell_state::ComposerSpelling,
}

impl ComposerUi {
    pub(super) fn new() -> Self {
        Self {
            mention_selected: 0,
            suggest: super::composer_suggest::SuggestUi::default(),
            emoji_words: Default::default(),
            inline_query_token: 0,
            inline_scroll: gpui_kit::ScrollHandle::new(),
            inline_return: None,
            preview_timer: None,
            prev_text: gpui_kit::SharedString::default(),
            markdown_revert: None,
            send_silent_once: false,
            send_effect_once: 0,
            send_menu: Default::default(),
            rich_draft: Default::default(),
            draft_restore_pending: false,
            remote_draft_seen: 0,
            shown_draft: None,
            placeholder_shown: None,
            send_morph: Default::default(),
            inline_bot_alert_shown: false,
            drop_paths: Vec::new(),
            drop_state: None,
            drop_preview: None,
            slow_mode_tick_chat: None,
            send_box: super::send_box_state::SendBoxUi::new(),
            text_limit: Default::default(),
            spelling: Default::default(),
        }
    }
}

/// The video editor's preview is a file on disk: it goes with the editor
/// (`close_video_editor`); the rest of the composer's state is the
/// account's only app-side.
impl super::on_screen::Farewell for ComposerUi {
    fn farewell(
        app: &mut super::app::QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<super::app::QuillApp>,
    ) {
        app.close_video_editor(cx);
    }
}
