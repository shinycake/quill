//! The members block of the group info panel: how many rows are drawn and
//! where the list is scrolled. Paging, the last page and the
//! administrators fallback live in the session's member caches
//! (`MemberAsk`, `SupergroupMembersFetch`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;

pub(crate) struct InlineMembersUi {
    /// Rows drawn so far; grows as the list scrolls.
    pub(super) shown: usize,
    pub(super) scroll: ScrollHandle,
}

impl crate::ui::on_screen::Farewell for InlineMembersUi {}

impl InlineMembersUi {
    pub(super) fn new() -> Self {
        Self {
            shown: quill::inline_members::ROWS_STEP,
            scroll: ScrollHandle::new(),
        }
    }

    /// Back to the first page for another group.
    pub(super) fn reset(&mut self) {
        self.shown = quill::inline_members::ROWS_STEP;
    }
}
