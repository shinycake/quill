//! The next page of the chat's posted and archived stories on the story
//! page, asked for when its list is scrolled near the end
//! (`ui/scroll_load.rs`). A page that comes back empty is not asked for
//! again on its own.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::scroll_load::{load_due, scrolled_near_end};
use quill::state::RequestPurpose;
use quill::story_page::{StoryPageOpState, story_page_op_label};

impl QuillApp {
    /// A page of `purpose` is on its way.
    fn story_page_loading(&self, purpose: RequestPurpose) -> bool {
        self.session()
            .and_then(|s| s.stories.page_op.as_ref())
            .is_some_and(|op| {
                op.state == StoryPageOpState::Checking && op.label == story_page_op_label(purpose)
            })
    }

    pub(super) fn archived_loading(&self) -> bool {
        self.story_page_loading(RequestPurpose::GetChatArchivedStories)
    }

    pub(super) fn chat_page_loading(&self) -> bool {
        self.story_page_loading(RequestPurpose::GetChatPostedToChatPageStories)
    }

    /// More posted stories exist than are loaded.
    pub(super) fn chat_page_has_more(&self) -> bool {
        let Some(chat) = self.stories.page.as_ref().map(|page| page.chat_id.0) else {
            return false;
        };
        self.session()
            .and_then(|s| s.stories.chat_page_stories.get(&chat))
            .is_some_and(|posted| {
                usize::try_from(posted.total_count).is_ok_and(|n| n > posted.story_ids.len())
            })
    }

    fn archived_loaded(&self) -> usize {
        let chat = self.stories.page.as_ref().map(|page| page.chat_id.0);
        chat.and_then(|chat| self.session()?.stories.archived_stories.get(&chat))
            .map_or(0, |archived| archived.story_ids.len())
    }

    fn chat_page_loaded(&self) -> usize {
        let chat = self.stories.page.as_ref().map(|page| page.chat_id.0);
        chat.and_then(|chat| self.session()?.stories.chat_page_stories.get(&chat))
            .map_or(0, |posted| posted.story_ids.len())
    }

    /// The archive section is near its end: ask for the next page.
    pub(super) fn story_archive_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let loaded = self.archived_loaded();
        let Some(page) = self.stories.page.as_ref() else {
            return;
        };
        if page.open_album.is_some()
            || !load_due(
                scrolled_near_end(&page.archive_scroll),
                self.picker_has_more(),
                self.archived_loading(),
            )
            || !page.archive_asked.allow(loaded, scrolled)
        {
            return;
        }
        self.load_more_archived_stories(cx);
    }

    /// The posted-stories section is near its end: ask for the next page.
    pub(super) fn story_posted_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let loaded = self.chat_page_loaded();
        let Some(page) = self.stories.page.as_ref() else {
            return;
        };
        if page.open_album.is_some()
            || !load_due(
                scrolled_near_end(&page.posted_scroll),
                self.chat_page_has_more(),
                self.chat_page_loading(),
            )
            || !page.posted_asked.allow(loaded, scrolled)
        {
            return;
        }
        self.load_more_chat_page_stories(cx);
    }
}
