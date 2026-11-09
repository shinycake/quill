//! Message menu, selection, links and per-message toggles in the history.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::*;
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
    /// The checklist task or poll option the open message menu was opened
    /// over (the whole message when none).
    pub(super) menu_item: quill::composer::ReplyItem,
    /// The task or option under the latest right-press, and where.
    pub(super) right_clicked_item: Option<(Point<Pixels>, quill::composer::ReplyItem)>,
    pub(super) link_tooltip: Option<super::entity_links::LinkTooltip>,
    /// The "Open this link?" box (`DialogKind::OpenLink`).
    pub(super) open_link_confirm: Option<super::entity_links::OpenLinkConfirm>,
    /// A clicked message entity (mention, hashtag, link…) waiting for
    /// render to act on it (`entity_links`).
    pub(super) pending_link: Option<super::entity_links::PendingLink>,
    /// A left press on a row that may become a drag selection, and the
    /// range it covers once it did (`selection_drag`).
    pub(super) selection_drag: Option<super::selection_drag::DragSelect>,
    /// The 15 ms pointer poll of a held press is running.
    pub(super) selection_drag_loop: bool,
    /// Keyboard focus of the history while selecting: a row press or a
    /// drag moves focus here, so Space, Up and Down act on the rows and
    /// not on the composer (tdesktop focuses `HistoryInner`).
    pub(super) history_focus: FocusHandle,
    /// Screen-reader focus among the history's messages (`history_a11y`).
    pub(super) a11y: super::history_a11y::HistoryA11y,
    /// tdesktop hover React / Unigram ReactionButton picker (emoji only).
    /// The message menu's reaction strip is expanded to every reaction.
    pub(super) reactions_expanded: bool,
    /// M1: swipe-to-reply press origin (chat, message, press x).
    pub(super) swipe_reply_start: Option<(ChatId, MessageId, Pixels)>,
    /// Phase 4.1: revealed text-entity spoilers, keyed by
    /// (chat id, message id, run index, is-caption block). Message ids are
    /// only unique within a chat, so the chat id is part of the key.
    pub(super) spoiler_revealed: HashSet<(i64, u64, u64, bool)>,
    /// Messages whose fact-check block is expanded (`(chat id, message
    /// id)`; tdesktop `Factcheck::expanded`).
    pub(super) fact_check_expanded: HashSet<(i64, i64)>,
    /// B1: one-time custom keyboards the user already tapped
    /// (`(chat_id, message_id)`), hidden locally after use.
    pub(super) dismissed_keyboards: std::collections::HashSet<(i64, i64)>,
    /// Keyboards the user hid with the composer's keyboard button.
    pub(super) collapsed_keyboards: std::collections::HashSet<(i64, i64)>,
    /// A bot request button's share dialog (`DialogKind::RequestShare`).
    pub(super) request_share: Option<super::request_share::RequestShare>,
    /// The location lookup behind a bot's location request.
    pub(super) request_locate: Option<Task<()>>,
    /// "About These Ads" box (`DialogKind::SponsoredAbout`) is open.
    pub(super) sponsored_about: Option<super::sponsored_about::SponsoredAbout>,
    /// Sponsored message ids the footer painted last frame, with the chat
    /// painted; reported to TDLib as viewed at the next frame start
    /// (`report_visible_sponsored`) when that chat is still open.
    pub(super) rendered_sponsored: std::cell::RefCell<Vec<(ChatId, i64)>>,
    /// The custom emoji card already on screen (it times itself out).
    pub(super) custom_emoji_card_seen: Option<quill::state::CustomEmojiPreview>,
    /// The custom emoji last tapped in this window: its card shows here,
    /// over the window of the tap (the session keeps one preview).
    pub(super) custom_emoji_tapped: Option<i64>,
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
            menu_item: quill::composer::ReplyItem::Message,
            right_clicked_item: None,
            link_tooltip: None,
            open_link_confirm: None,
            pending_link: None,
            selection_drag: None,
            selection_drag_loop: false,
            history_focus: cx.focus_handle(),
            a11y: Default::default(),
            reactions_expanded: false,
            swipe_reply_start: None,
            spoiler_revealed: HashSet::new(),
            fact_check_expanded: HashSet::new(),
            dismissed_keyboards: std::collections::HashSet::new(),
            collapsed_keyboards: std::collections::HashSet::new(),
            request_share: None,
            request_locate: None,
            sponsored_about: None,
            rendered_sponsored: std::cell::RefCell::new(Vec::new()),
            custom_emoji_card_seen: None,
            custom_emoji_tapped: None,
            poll_voters_dialog: None,
            poll_add_option: None,
            fact_check_dialog: None,
        }
    }
}

/// Nothing to say: its state is the account's only app-side.
/// The Block from replies box keeps its target in the session
/// (`users_state.replies_block`), which goes to the background with the
/// account.
impl super::on_screen::Farewell for MessageUi {
    fn farewell(
        app: &mut super::app::QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<super::app::QuillApp>,
    ) {
        if app
            .session()
            .is_some_and(|session| session.users_state.replies_block.is_some())
        {
            app.close_replies_block(cx);
        }
    }
}
