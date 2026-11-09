//! The members block of the group info panel: search, paging and the last
//! page shown while a refetch runs.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::envelope::ParsedChatMember;
use std::cell::RefCell;

/// The last good page, drawn while a membership change makes the session
/// drop its cache and refetch it.
pub(super) struct MembersSnapshot {
    pub(super) chat: ChatId,
    pub(super) members: Vec<ParsedChatMember>,
    pub(super) total: i32,
}

pub(crate) struct InlineMembersUi {
    /// Rows drawn so far; grows as the list scrolls.
    pub(super) shown: usize,
    pub(super) scroll: ScrollHandle,
    /// The group the block is showing.
    pub(super) chat: Option<ChatId>,
    /// A next-page request at this offset failed; do not repeat it.
    pub(super) failed_offset: Option<i32>,
    /// The request for the page at `failed_offset` failed (the session's
    /// `member_more_failed`): the next scroll asks for it again. A page
    /// that came back empty stays at `failed_offset`.
    pub(super) retry_at_scroll: bool,
    /// The Administrators fallback was asked for this group since the block
    /// opened or Retry was pressed: a failed answer is not asked again on
    /// every frame.
    pub(super) admins_asked: bool,
    pub(super) snapshot: RefCell<Option<MembersSnapshot>>,
}

impl crate::ui::on_screen::Farewell for InlineMembersUi {}

impl InlineMembersUi {
    pub(super) fn new() -> Self {
        Self {
            shown: quill::inline_members::ROWS_STEP,
            scroll: ScrollHandle::new(),
            chat: None,
            failed_offset: None,
            retry_at_scroll: false,
            admins_asked: false,
            snapshot: RefCell::new(None),
        }
    }

    /// Back to the first page for another group.
    pub(super) fn reset(&mut self, chat: Option<ChatId>) {
        self.shown = quill::inline_members::ROWS_STEP;
        self.chat = chat;
        self.failed_offset = None;
        self.retry_at_scroll = false;
        self.admins_asked = false;
        self.snapshot.replace(None);
    }
}
