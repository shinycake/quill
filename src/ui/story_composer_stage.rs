//! The story composer's stage upkeep: each stage (`ComposerStage`) leaves
//! with the answer to the request it sent, never with another composer's
//! or another call's answer.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::Context;
use quill::ids::RequestId;
use quill::state::{StoryManageOutcome, StoryPostOutcome};
use quill::story_composer::ComposerStage;

impl QuillApp {
    /// The panel that sent management call `request` shows why it failed
    /// under its story, even when another call took the status slot since
    /// (`StoryManageState::failure` keeps each call's own answer).
    pub(super) fn show_story_manage_failure(
        &mut self,
        request: RequestId,
        story: Option<(i64, i32)>,
    ) {
        let Some((chat_id, story_id)) = story else {
            return;
        };
        let session = match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        };
        if let Some(session) = session
            && let Some(reason) = session.stories.manage.failure(request).map(str::to_string)
        {
            session
                .stories
                .manage
                .fail_locally(chat_id, story_id, reason);
        }
    }

    /// Per frame: move the composer on once the request of its stage is
    /// answered. main-only: the story composer is the main window's overlay.
    pub(super) fn story_composer_upkeep(&mut self, cx: &mut Context<Self>) {
        match self.stories.composer.stage.clone() {
            ComposerStage::Editing | ComposerStage::Posted => {}
            // Phase 9.3: the composer sent `canPostStory` — once its own
            // answer lands, either post (eligible) or surface the reason.
            ComposerStage::Checking { request, target } => {
                let answer = self
                    .session()
                    .and_then(|s| s.stories.post.check.answer_to(request).cloned());
                if let Some(answer) = answer {
                    self.stories.composer.stage = ComposerStage::Editing;
                    self.story_composer_after_check(target, answer, cx);
                }
            }
            // Phase 9.3: `postStory` was sent — its outcome ends the stage:
            // a posted story says so, a failed one shows why.
            ComposerStage::Posting { request } => {
                let outcome = self
                    .session()
                    .map(|s| s.stories.post.outcome.clone())
                    .filter(|outcome| outcome.request() == Some(request));
                match outcome {
                    Some(StoryPostOutcome::Succeeded { .. }) => {
                        self.stories.composer.stage = ComposerStage::Posted;
                        cx.notify();
                    }
                    Some(StoryPostOutcome::Failed { message, .. }) => {
                        self.stories.composer.stage = ComposerStage::Editing;
                        self.stories.composer.local_error = Some(message);
                        cx.notify();
                    }
                    Some(StoryPostOutcome::Posting { .. } | StoryPostOutcome::None) | None => {}
                }
            }
            // Phase 9.5: `editStory` was sent — its own answer closes the
            // composer on success (the edited story arrives via
            // `updateStory`) or shows the failure; another management call
            // taking the slot meanwhile changes nothing here.
            ComposerStage::Saving { request } => {
                let outcome = self
                    .session()
                    .map(|s| s.stories.manage.outcome(request))
                    .unwrap_or(StoryManageOutcome::Lost);
                match outcome {
                    StoryManageOutcome::InFlight => {}
                    StoryManageOutcome::Saved => self.close_story_composer(cx),
                    StoryManageOutcome::Failed => {
                        let reason = self
                            .session()
                            .and_then(|s| s.stories.manage.failure(request).map(str::to_string));
                        self.stories.composer.stage = ComposerStage::Editing;
                        self.stories.composer.local_error = reason;
                        cx.notify();
                    }
                    StoryManageOutcome::Lost => {
                        self.stories.composer.stage = ComposerStage::Editing;
                        cx.notify();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "story_composer_stage_tests.rs"]
mod tests;
