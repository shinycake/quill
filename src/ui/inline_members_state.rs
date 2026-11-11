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

/// The next-page request of the members list. A page is asked once per
/// offset: not again while it is out or after it failed, until the user
/// scrolls again (a failing server is not asked in a loop).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Paging {
    /// Nothing asked since the block opened.
    Idle,
    /// The page at `offset` was asked for.
    Asked { offset: i32 },
    /// The page at `offset` failed (the request, or its send): the next
    /// scroll asks for it again.
    Failed { offset: i32 },
}

impl Paging {
    /// The offset that is not asked again.
    pub(super) fn blocked(self) -> Option<i32> {
        match self {
            Paging::Idle => None,
            Paging::Asked { offset } | Paging::Failed { offset } => Some(offset),
        }
    }
}

/// The Administrators fallback (members hidden from the viewer): asked once
/// for a group since the block opened or Retry was pressed, so a failing
/// list is not asked on every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum AdminsFallback {
    #[default]
    NotAsked,
    Asked(ChatId),
}

pub(crate) struct InlineMembersUi {
    /// Rows drawn so far; grows as the list scrolls.
    pub(super) shown: usize,
    pub(super) scroll: ScrollHandle,
    /// The group the block is showing.
    pub(super) chat: Option<ChatId>,
    /// Where the next-page request stands.
    pub(super) paging: Paging,
    pub(super) admins: AdminsFallback,
    pub(super) snapshot: RefCell<Option<MembersSnapshot>>,
}

impl crate::ui::on_screen::Farewell for InlineMembersUi {}

impl InlineMembersUi {
    pub(super) fn new() -> Self {
        Self {
            shown: quill::inline_members::ROWS_STEP,
            scroll: ScrollHandle::new(),
            chat: None,
            paging: Paging::Idle,
            admins: AdminsFallback::NotAsked,
            snapshot: RefCell::new(None),
        }
    }

    /// Back to the first page for another group.
    pub(super) fn reset(&mut self, chat: Option<ChatId>) {
        self.shown = quill::inline_members::ROWS_STEP;
        self.chat = chat;
        self.refetch();
        self.snapshot.replace(None);
    }

    /// The cached pages are gone (membership changed, Retry, a new open):
    /// the next scroll or frame asks for every page again.
    pub(super) fn refetch(&mut self) {
        self.paging = Paging::Idle;
        self.admins = AdminsFallback::NotAsked;
    }
}
