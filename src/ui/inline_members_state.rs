//! The members block of the group info panel: search, paging and the last
//! page shown while a refetch runs.

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
    pub(super) snapshot: RefCell<Option<MembersSnapshot>>,
}

impl InlineMembersUi {
    pub(super) fn new() -> Self {
        Self {
            shown: quill::inline_members::ROWS_STEP,
            scroll: ScrollHandle::new(),
            chat: None,
            failed_offset: None,
            snapshot: RefCell::new(None),
        }
    }

    /// Back to the first page for another group.
    pub(super) fn reset(&mut self, chat: Option<ChatId>) {
        self.shown = quill::inline_members::ROWS_STEP;
        self.chat = chat;
        self.failed_offset = None;
        self.snapshot.replace(None);
    }
}
