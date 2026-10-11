//! Message menu, selection, links and per-message toggles in the history.

use super::*;
use gpui_kit::*;
use quill::composer::DeleteConfirm;
use quill::ids::ChatId;
use quill::ids::MessageId;
use std::collections::HashSet;

pub(crate) struct MessageUi {
    /// M1: right-click context menu target + window position.
    pub(super) menu: Option<MessageMenuState>,
    /// Message text selected when the message menu opened, if the
    /// selection lies in that message (Quote & Reply, Copy Selected Text).
    pub(super) menu_selection: Option<String>,
    /// Page, report and sticker-set dialogs of the message menu's extras.
    pub(super) menu_ui: super::message_menu_ui::MessageMenuUi,
    /// The link the open message menu was opened over.
    pub(super) menu_link: Option<quill::text::LinkTarget>,
    /// The link under the latest right-press, and where it was pressed.
    pub(super) right_clicked_link: Option<(Point<Pixels>, quill::text::LinkTarget)>,
    pub(super) link_tooltip: Option<super::entity_links::LinkTooltip>,
    /// The "Open this link?" box (`DialogKind::OpenLink`).
    pub(super) open_link_confirm: Option<super::entity_links::OpenLinkConfirm>,
    /// The copy menu of a phone number, card number or date.
    pub(super) link_popup: Option<super::entity_links::LinkPopup>,
    /// A clicked message entity (mention, hashtag, link…) waiting for
    /// render to act on it (`entity_links`).
    pub(super) pending_link: Option<super::entity_links::PendingLink>,
    /// Last row clicked in selection mode: the Shift+click range anchor.
    pub(super) selection_anchor: Option<MessageId>,
    /// A left press on a row that may become a drag selection, and the
    /// range it covers once it did (`selection_drag`).
    pub(super) selection_drag: Option<super::selection_drag::DragSelect>,
    /// The 15 ms pointer poll of a held press is running.
    pub(super) selection_drag_loop: bool,
    /// The row keyboard selection acts on (Ctrl+Space, Up / Down).
    pub(super) selection_focus: Option<MessageId>,
    /// Keyboard focus of the history while selecting: a row press or a
    /// drag moves focus here, so Space, Up and Down act on the rows and
    /// not on the composer (tdesktop focuses `HistoryInner`).
    pub(super) history_focus: FocusHandle,
    /// tdesktop hover React / Unigram ReactionButton picker (emoji only).
    /// The message menu's reaction strip is expanded to every reaction.
    pub(super) reactions_expanded: bool,
    /// Delete confirm (tdesktop `DeleteMessagesBox` / Unigram popup).
    pub(super) pending_delete: Option<DeleteConfirm>,
    /// B4: pending "Stop poll" confirm (`stopPoll`, schema 1.8.67 line
    /// 12953) — (chat, message, is_quiz) for the warning copy. TGX
    /// `StopPollWarn` / `StopQuizWarn`: nobody can vote afterwards and
    /// the action can't be undone.
    pub(super) pending_stop_poll: Option<(ChatId, MessageId, bool)>,
    /// M1: swipe-to-reply press origin (chat, message, press x).
    pub(super) swipe_reply_start: Option<(ChatId, MessageId, Pixels)>,
    /// Phase 4.1: revealed text-entity spoilers, keyed by
    /// (chat id, message id, run index, is-caption block). Message ids are
    /// only unique within a chat, so the chat id is part of the key.
    pub(super) spoiler_revealed: HashSet<(i64, u64, u64, bool)>,
    /// B1: one-time custom keyboards the user already tapped
    /// (`(chat_id, message_id)`), hidden locally after use.
    pub(super) dismissed_keyboards: std::collections::HashSet<(i64, i64)>,
    /// Keyboards the user hid with the composer's keyboard button.
    pub(super) collapsed_keyboards: std::collections::HashSet<(i64, i64)>,
    /// A bot request button's share dialog (`DialogKind::RequestShare`).
    pub(super) request_share: Option<super::request_share::RequestShare>,
    /// "About These Ads" box (`DialogKind::SponsoredAbout`) is open.
    pub(super) sponsored_about: Option<super::sponsored_about::SponsoredAbout>,
    /// Sponsored message ids the footer painted last frame; reported to
    /// TDLib as viewed at the next frame start (`report_visible_sponsored`).
    pub(super) rendered_sponsored: std::cell::RefCell<Vec<i64>>,
    /// The custom emoji card already on screen (it times itself out).
    pub(super) custom_emoji_card_seen: Option<quill::state::CustomEmojiPreview>,
    /// B4: poll voter-list viewer.
    pub(super) poll_voters_dialog: Option<PollVotersDialog>,
    /// B15: the inline "Add an Option" panel (`addPollOption`).
    pub(super) poll_add_option: Option<PollAddOption>,
    /// The message menu's "Add Fact Check" / "Edit Fact Check" dialog.
    pub(super) fact_check_dialog: Option<super::fact_check::FactCheckDialog>,
}

impl MessageUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            menu: None,
            menu_selection: None,
            menu_ui: super::message_menu_ui::MessageMenuUi::new(window, cx),
            menu_link: None,
            right_clicked_link: None,
            link_tooltip: None,
            open_link_confirm: None,
            link_popup: None,
            pending_link: None,
            selection_anchor: None,
            selection_drag: None,
            selection_drag_loop: false,
            selection_focus: None,
            history_focus: cx.focus_handle(),
            reactions_expanded: false,
            pending_delete: None,
            pending_stop_poll: None,
            swipe_reply_start: None,
            spoiler_revealed: HashSet::new(),
            dismissed_keyboards: std::collections::HashSet::new(),
            collapsed_keyboards: std::collections::HashSet::new(),
            request_share: None,
            sponsored_about: None,
            rendered_sponsored: std::cell::RefCell::new(Vec::new()),
            custom_emoji_card_seen: None,
            poll_voters_dialog: None,
            poll_add_option: None,
            fact_check_dialog: None,
        }
    }
}
