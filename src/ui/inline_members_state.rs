//! The members block of the group info panel: search, paging and the last
//! page shown while a refetch runs.

use gpui_kit::component::input::TextareaState;
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
    /// The search field (created when a group panel first opens).
    pub(super) search: Option<Entity<TextareaState>>,
    pub(super) search_open: bool,
    /// The text the list is narrowed by, trimmed.
    pub(super) query: String,
    /// Bumped on each keystroke; a debounce timer only acts on the latest.
    pub(super) token: u64,
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
            search: None,
            search_open: false,
            query: String::new(),
            token: 0,
            shown: quill::inline_members::ROWS_STEP,
            scroll: ScrollHandle::new(),
            chat: None,
            failed_offset: None,
            snapshot: RefCell::new(None),
        }
    }

    /// Back to the first page with no search, for another group.
    pub(super) fn reset(&mut self, chat: Option<ChatId>) {
        self.search_open = false;
        self.query.clear();
        self.token += 1;
        self.shown = quill::inline_members::ROWS_STEP;
        self.chat = chat;
        self.failed_offset = None;
        self.snapshot.replace(None);
    }
}
