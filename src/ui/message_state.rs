//! Message menu, selection, links and per-message toggles in the history.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::ids::MessageId;
use std::collections::HashSet;

/// The text selected when the message menu opened, if it lies in that
/// message (Quote & Reply, Copy Selected Text).
#[derive(Clone, Debug, Default)]
pub(super) struct MenuSelection {
    pub(super) text: String,
    /// Which occurrence of `text` in the message the selection started at
    /// (0 for the first); where Quote & Reply points.
    pub(super) nth: usize,
}

/// What the open message menu was opened over.
#[derive(Clone, Debug)]
pub(super) struct MenuTarget {
    /// The link the menu was opened over.
    pub(super) link: Option<quill::text::LinkTarget>,
    /// The checklist task or poll option it was opened over (the whole
    /// message when none).
    pub(super) item: quill::composer::ReplyItem,
    pub(super) selection: Option<MenuSelection>,
    /// tdesktop hover React / Unigram ReactionButton picker (emoji only):
    /// the menu's reaction strip is expanded to every reaction.
    pub(super) reactions_expanded: bool,
}

impl Default for MenuTarget {
    fn default() -> Self {
        Self {
            link: None,
            item: quill::composer::ReplyItem::Message,
            selection: None,
            reactions_expanded: false,
        }
    }
}

/// The open message menu: where it is, the page it shows and what it was
/// opened over. One value, so closing the menu drops all of it.
#[derive(Clone, Debug)]
pub(super) struct OpenMenu {
    /// The message and the window position.
    pub(super) at: super::menu_states::MessageMenuState,
    /// Which page of the menu shows (a reactor list, "Save to...").
    pub(super) page: super::message_menu_ui::MessageMenuPage,
    /// What the menu was opened over: a link, a task or option, a text
    /// selection.
    pub(super) target: MenuTarget,
}

pub(crate) struct MessageUi {
    /// M1: right-click context menu (target, page and window position).
    pub(super) menu: Option<OpenMenu>,
    /// Report and sticker-set dialogs of the message menu's extras.
    pub(super) menu_ui: super::message_menu_ui::MessageMenuUi,
    /// The link under the latest right-press, and where it was pressed.
    pub(super) right_clicked_link: Option<(Point<Pixels>, quill::text::LinkTarget)>,
    /// The task or option under the latest right-press, and where.
    pub(super) right_clicked_item: Option<(Point<Pixels>, quill::composer::ReplyItem)>,
    /// The "Open this link?" box (`DialogKind::OpenLink`).
    pub(super) open_link_confirm: Option<super::entity_links::OpenLinkConfirm>,
    /// A clicked message entity (mention, hashtag, link…) waiting for
    /// render to act on it (`entity_links`).
    pub(super) pending_link: Option<super::entity_links::PendingLink>,
    /// A left press on a row that may become a drag selection, and the
    /// range it covers once it did (`selection_drag`).
    pub(super) selection_drag: Option<super::selection_drag::DragSelect>,
    /// Keyboard focus of the history while selecting: a row press or a
    /// drag moves focus here, so Space, Up and Down act on the rows and
    /// not on the composer (tdesktop focuses `HistoryInner`).
    pub(super) history_focus: FocusHandle,
    /// Screen-reader focus among the history's messages (`history_a11y`).
    pub(super) a11y: super::history_a11y::HistoryA11y,
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
    /// Sponsored message ids painted inside the history's viewport last
    /// frame, with the chat painted; reported to TDLib as viewed at the next
    /// frame start (`report_visible_sponsored`) when that chat is still open.
    pub(super) rendered_sponsored: std::rc::Rc<std::cell::RefCell<Vec<(ChatId, i64)>>>,
    /// The custom emoji card already on screen, with the tap it was shown
    /// for (it times itself out; a new tap of the same emoji starts the
    /// timer over).
    pub(super) custom_emoji_card_seen: Option<(quill::state::CustomEmojiPreview, u64)>,
    /// Counts taps of custom emoji, so each one has its own card timer.
    pub(super) custom_emoji_tap_count: u64,
    /// The custom emoji last tapped in this window: its card shows here,
    /// over the window of the tap (the session keeps one preview).
    pub(super) custom_emoji_tapped: Option<i64>,
    /// B4: poll voter-list viewer.
    pub(super) poll_voters_dialog: Option<PollVotersDialog>,
    /// B15: the inline "Add an Option" panel (`addPollOption`).
    pub(super) poll_add_option: Option<PollAddOption>,
    /// The message menu's "Add Fact Check" / "Edit Fact Check" dialog.
    pub(super) fact_check_dialog: Option<super::fact_check::FactCheckDialog>,
    /// Saved Messages: "Add Name" / "Edit Name" for a tag, in the window
    /// whose tag menu asked.
    pub(super) saved_tag_dialog: Option<super::saved_sublists::SavedTagDialog>,
}

impl MessageUi {
    /// Open the menu at `at`, on its main page and over nothing.
    pub(super) fn set_menu(&mut self, at: super::menu_states::MessageMenuState) {
        self.menu = Some(OpenMenu {
            at,
            page: Default::default(),
            target: MenuTarget::default(),
        });
    }

    /// Where the open menu is.
    pub(super) fn menu_at(&self) -> Option<super::menu_states::MessageMenuState> {
        self.menu.as_ref().map(|open| open.at)
    }

    /// The page the open menu shows (the main one when none is open).
    pub(super) fn menu_page(&self) -> super::message_menu_ui::MessageMenuPage {
        self.menu.as_ref().map(|open| open.page).unwrap_or_default()
    }

    /// Turn the open menu to `page`; nothing when it has closed.
    pub(super) fn set_menu_page(&mut self, page: super::message_menu_ui::MessageMenuPage) {
        if let Some(open) = self.menu.as_mut() {
            open.page = page;
        }
    }

    /// What the open menu was opened over.
    pub(super) fn menu_target(&self) -> Option<&MenuTarget> {
        self.menu.as_ref().map(|open| &open.target)
    }

    /// The task or option the open menu is over (the whole message when
    /// none, or when no menu is open).
    pub(super) fn menu_item(&self) -> quill::composer::ReplyItem {
        self.menu_target()
            .map_or(quill::composer::ReplyItem::Message, |t| t.item.clone())
    }

    /// Record what the open menu was opened over.
    pub(super) fn set_menu_target(&mut self, target: MenuTarget) {
        if let Some(open) = self.menu.as_mut() {
            open.target = target;
        }
    }

    /// Expand the open menu's reaction strip to every reaction.
    pub(super) fn expand_menu_reactions(&mut self, expanded: bool) {
        if let Some(open) = self.menu.as_mut() {
            open.target.reactions_expanded = expanded;
        }
    }

    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            menu: None,
            menu_ui: super::message_menu_ui::MessageMenuUi::new(window, cx),
            right_clicked_link: None,
            right_clicked_item: None,
            open_link_confirm: None,
            pending_link: None,
            selection_drag: None,
            history_focus: cx.focus_handle(),
            a11y: Default::default(),
            swipe_reply_start: None,
            spoiler_revealed: HashSet::new(),
            dismissed_keyboards: std::collections::HashSet::new(),
            collapsed_keyboards: std::collections::HashSet::new(),
            request_share: None,
            sponsored_about: None,
            rendered_sponsored: Default::default(),
            custom_emoji_card_seen: None,
            custom_emoji_tap_count: 0,
            custom_emoji_tapped: None,
            poll_voters_dialog: None,
            poll_add_option: None,
            fact_check_dialog: None,
            saved_tag_dialog: None,
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
        super::emoji_effect::forget_account_notes();
        if app
            .session()
            .is_some_and(|session| session.users_state.replies_block.is_some())
        {
            app.close_replies_block(cx);
        }
    }
}
