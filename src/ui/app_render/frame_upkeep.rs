//! Per-frame upkeep that only the main window's `render` runs: things the
//! main window alone draws or owns (notifications, restored chat windows,
//! stories, sign-in, the info panel's earn pages). What every window needs
//! is `per_window_upkeep` (`window_upkeep.rs`), called from here with the
//! main window's state in place. Every other step here says why it is the
//! main window's with `// main-only: <why>` in the comment right above it
//! (`scripts/check-window-scope.py`). Only notification delivery runs while
//! Quill is locked.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};
use quill::state::{StoryManageOutcome, StoryPostOutcome};

impl QuillApp {
    pub(super) fn frame_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(feature = "frame-profiler")]
        self.frame_profile_tick(window);
        // main-only: notification clicks, other accounts and delivery are the
        // app's; a chat they open goes through the router (`route_chat_open`).
        self.flush_notifications(window, cx);
        // Locked: the steps below turn stored requests and answers into
        // something shown or started (a box, a viewer, a sign-in, a page).
        // Their state waits for the first frame after the unlock, as a
        // chat window's locked frame runs none of them either
        // (docs/decisions/lock-is-a-render-state.md, "What runs while
        // locked"; `scripts/lock_surfaces.py` checks this order).
        if self.lock_waits() {
            return;
        }
        // What every window's frame runs, here with the main window's
        // state in place (`window_upkeep`), as the window that asks.
        self.asked_in_place(|this| this.per_window_upkeep(window, cx));
        // The chat windows open at the last run come back once the
        // account is ready (`chat_window::saved`).
        // main-only: it opens the chat windows themselves.
        self.restore_chat_windows(window, cx);
        // Revenue and affiliate pages: the amount field, the setup
        // sliders, program photos and pages, profile balances.
        // main-only: they are pages of the session's info panel, which only
        // the main window's right column draws (chat windows have none).
        self.earn_frame_upkeep(window, cx);
        // main-only: the composer button's blink follows the main window's
        // field; the box, the shortcut, the style notices and their emoji
        // are their window's (`ai_compose_window_upkeep`).
        self.ai_compose_frame_upkeep(cx);
        // main-only: stories open in the main window.
        self.resolve_pending_story_open(window, cx);
        // main-only: mini apps open over the main window.
        self.run_pending_mini_app_action(window, cx);
        // main-only: the story tray, viewer and composer are the main
        // window's (`story_frame_upkeep`).
        self.story_frame_upkeep(cx);
        // main-only: the sign-in screen is the main window's.
        self.sync_signin(window, cx);
        // main-only: the sign-in forms are the main window's.
        self.sync_auth_forms(window, cx);
        // main-only: the two-step boxes open in the main window.
        self.sync_twofa_forms(window, cx);
        // main-only: the chat list and its multi-select are the main
        // window's (chat windows have none).
        self.end_selection_left_list();
    }

    /// The stories' follow-ups: refreshed trays, the composer's check,
    /// post and save answers, the viewer's editors and a deleted story.
    /// Only the main window draws the story tray, viewer and composer.
    fn story_frame_upkeep(&mut self, cx: &mut Context<Self>) {
        // Phase 9.2: the `updateStoryPostSucceeded` reducer queued poster
        // chats whose active stories should be refreshed (an own story
        // posted from another client appears in the tray this way).
        if let Some(live) = self.live.as_mut() {
            let chats: Vec<i64> = live.driver.session.stories.tray_refresh.drain().collect();
            for chat_id in chats {
                let _ = live.driver.get_chat_active_stories(ChatId(chat_id));
            }
        }
        // Phase 9.3: the composer sent `canPostStory` — once the answer
        // lands, either post (eligible) or surface the reason in the
        // composer. Eligibility is re-checked on every Post press.
        if self.stories.composer.check_sent {
            let answered = self.session().is_some_and(|session| {
                session.stories.post.eligibility.is_some()
                    || session.stories.post.check_error.is_some()
            });
            if answered {
                self.stories.composer.check_sent = false;
                self.story_composer_after_check(cx);
            }
        }
        // Phase 9.3: `postStory` was sent (`post_sent`) — once its
        // answer moves `story_post.outcome` out of `None` the outcome
        // owns the busy state again and the flag clears.
        if self.stories.composer.post_sent
            && self.session().is_some_and(|session| {
                !matches!(session.stories.post.outcome, StoryPostOutcome::None)
            })
        {
            self.stories.composer.post_sent = false;
        }
        // Phase 9.5: `editStory` was sent (`save_sent`) — once
        // `story_manage.pending` clears, success closes the composer
        // (the edited story arrives via `updateStory`); failure
        // surfaces `story_manage.error` in the composer.
        if self.stories.composer.save_sent
            && self
                .session()
                .is_some_and(|session| !session.stories.manage.pending)
        {
            self.stories.composer.save_sent = false;
            let failed = self.session().and_then(|s| s.stories.manage.error.clone());
            match failed {
                Some(error) => self.stories.composer.local_error = Some(error),
                None => self.close_story_composer(cx),
            }
        }
        // Phase 9.5: the viewer cover editor / privacy editor sent a
        // management call. Its own answer (matched by request id) closes
        // the panel on success or leaves it open showing the error; a
        // call let go without one just ends the wait.
        let outcome = |this: &Self, request: Option<quill::ids::RequestId>| {
            request.and_then(|id| {
                this.session()
                    .map(|session| session.stories.manage.outcome(id))
            })
        };
        let cover = outcome(self, self.stories.viewer_ui().cover_request);
        if let Some(done) = cover.filter(|o| *o != StoryManageOutcome::InFlight) {
            self.stories.with_viewer_ui(|open| {
                open.cover_request = None;
                if done == StoryManageOutcome::Saved {
                    open.cover = None;
                }
            });
        }
        let privacy = outcome(self, self.stories.viewer_ui().privacy_request);
        if let Some(done) = privacy.filter(|o| *o != StoryManageOutcome::InFlight) {
            self.stories.with_viewer_ui(|open| {
                open.privacy_request = None;
                if done == StoryManageOutcome::Saved {
                    open.privacy_edit = None;
                }
            });
        }
        // Phase 9.2: a story that vanished from the cache while being
        // viewed was deleted (`updateStoryDeleted`) — close the viewer.
        let current_deleted = self.stories.viewer.current().is_some_and(|item| {
            self.session().is_some_and(|session| {
                !session
                    .stories
                    .stories
                    .contains_key(&(item.chat_id.0, item.story_id))
            })
        });
        if current_deleted {
            // Its reply, and a reply being recorded, end with it.
            // main-only: only the main window draws the story viewer.
            self.release_story_viewer(cx);
            self.set_note(StatusNote::silent(tr(keys::Q_REST_STORY_DELETED)));
        }
        self.follow_story_tray(cx);
        self.story_viewers_scrolled(cx);
    }
}
