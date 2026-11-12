//! What the story page does when the mutation it waits on is answered
//! (`PageWait`): the create form clears, the picker closes, a deleted
//! album's view goes. A refused, failed or lost call changes nothing but
//! the status line, so the ticked stories and the typed name survive.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::story_page::PageWait;
use quill::ids::RequestId;
use quill::story_page::{StoryPageOpState, StoryPageOps};

/// How the calls of one wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answer {
    /// All of them succeeded.
    Succeeded,
    /// One failed, or its op is gone (the page was left, the account went).
    Refused,
}

/// How `requests` ended in `ops`; `None` while at least one is still on its
/// way.
pub(super) fn answer(ops: &StoryPageOps, requests: &[RequestId]) -> Option<Answer> {
    let mut refused = false;
    for request in requests {
        match ops.get(*request).map(|op| &op.state) {
            Some(StoryPageOpState::Sending | StoryPageOpState::Checking) => {
                return None;
            }
            Some(StoryPageOpState::Succeeded) => {}
            Some(StoryPageOpState::Failed(_)) | None => refused = true,
        }
    }
    Some(if refused {
        Answer::Refused
    } else {
        Answer::Succeeded
    })
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
            PageWait::SavingPicker { requests, .. }
            | PageWait::SavingPickerPartly { requests, .. } => requests.clone(),
        };
        let waits = !matches!(page.wait, PageWait::Idle);
        let Some(session) = self.session() else {
            return;
        };
        let delete_answer = deleting.map(|request| answer(&session.stories.page_ops, &[request]));
        let answered = waits.then(|| answer(&session.stories.page_ops, &requests));
        // A call still out answers nothing: no redraw asks for the next frame.
        if delete_answer.flatten().is_none() && answered.flatten().is_none() {
            return;
        }
        let Some(page) = self.stories.page.as_mut() else {
            return;
        };
        match delete_answer.flatten() {
            Some(Answer::Succeeded) => page.view = AlbumView::List,
            Some(Answer::Refused) => {
                page.view = page
                    .view
                    .open_album()
                    .map_or(AlbumView::List, AlbumView::album);
            }
            None => {}
        }
        if let Some(answered) = answered.flatten() {
            let wait = std::mem::replace(&mut page.wait, PageWait::Idle);
            if answered == Answer::Succeeded {
                // What the answer acts on is what asked: a picker the user
                // has moved on from is left as it is.
                let picker_target = page.picker.as_ref().map(|picker| picker.target);
                match wait {
                    PageWait::Idle => {}
                    PageWait::Creating(_) => {
                        if picker_target == Some(AlbumPickerTarget::NewAlbum) {
                            page.picker = None;
                        }
                        page.new_album_name
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    PageWait::SavingPicker { album_id, .. } => {
                        if picker_target == Some(AlbumPickerTarget::Album(album_id)) {
                            page.picker = None;
                        }
                    }
                    PageWait::SavingPickerPartly {
                        album_id, added, ..
                    } => {
                        if let Some(picker) = page
                            .picker
                            .as_mut()
                            .filter(|picker| picker.target == AlbumPickerTarget::Album(album_id))
                        {
                            picker.album_holds(&added);
                        }
                    }
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
