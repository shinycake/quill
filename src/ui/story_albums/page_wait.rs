//! What the story page does when the mutation it waits on is answered
//! (`PageWait`): the create form clears, the picker closes, a deleted
//! album's view goes. A refused, failed or lost call changes nothing but
//! the status line, so the ticked stories and the typed name survive.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::story_page::PageWait;
use quill::ids::RequestId;
use quill::story_page::{StoryPageOpState, StoryPageOps};

/// Where the calls of one wait stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answer {
    /// At least one is still on its way.
    Waiting,
    /// All of them succeeded.
    Succeeded,
    /// One failed, or its op is gone (the page was left, the account went).
    Refused,
}

/// The state of `requests` in `ops`.
pub(super) fn answer(ops: &StoryPageOps, requests: &[RequestId]) -> Answer {
    let mut refused = false;
    for request in requests {
        match ops.get(*request).map(|op| &op.state) {
            Some(StoryPageOpState::Sending | StoryPageOpState::Checking) => {
                return Answer::Waiting;
            }
            Some(StoryPageOpState::Succeeded) => {}
            Some(StoryPageOpState::Failed(_)) | None => refused = true,
        }
    }
    if refused {
        Answer::Refused
    } else {
        Answer::Succeeded
    }
}

impl QuillApp {
    /// Per frame: settle the page's wait once its calls are answered.
    /// main-only: the story page is the main window's overlay.
    pub(in crate::ui) fn story_page_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = self.stories.page.as_ref() else {
            return;
        };
        let deleting = page.view.deleting();
        let requests = match &page.wait {
            PageWait::Idle => Vec::new(),
            PageWait::Creating(request) => vec![*request],
            PageWait::SavingPicker(requests) => requests.clone(),
        };
        let waits = !matches!(page.wait, PageWait::Idle);
        let Some(session) = self.session() else {
            return;
        };
        let delete_answer = deleting.map(|request| answer(&session.stories.page_ops, &[request]));
        let answered = waits.then(|| answer(&session.stories.page_ops, &requests));
        if delete_answer.is_none() && answered.is_none() {
            return;
        }
        let Some(page) = self.stories.page.as_mut() else {
            return;
        };
        match delete_answer {
            Some(Answer::Succeeded) => page.view = AlbumView::List,
            Some(Answer::Refused) => {
                page.view = page
                    .view
                    .open_album()
                    .map_or(AlbumView::List, AlbumView::album);
            }
            Some(Answer::Waiting) | None => {}
        }
        if let Some(answered) = answered.filter(|answered| *answered != Answer::Waiting) {
            let wait = std::mem::replace(&mut page.wait, PageWait::Idle);
            if answered == Answer::Succeeded {
                match wait {
                    PageWait::Idle => {}
                    PageWait::Creating(_) => {
                        page.picker = None;
                        page.new_album_name
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    PageWait::SavingPicker(_) => page.picker = None,
                }
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "page_wait_tests.rs"]
mod tests;
