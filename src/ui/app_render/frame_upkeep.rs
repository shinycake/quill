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
use quill::state::StoryManageOutcome;

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
        // main-only: Settings and its Privacy page are the main window's.
        self.sync_check_password(window, cx);
        // Revenue and affiliate pages: the amount field, the setup
        // sliders, program photos and pages, profile balances.
        // main-only: they are pages of the session's info panel, which only
        // the main window's right column draws (chat windows have none).
        self.earn_frame_upkeep(window, cx);
        // main-only: the "Stop and quit?" alert is the main window's.
        self.quit_gate_upkeep(window, cx);
        // main-only: stories open in the main window.
        self.resolve_pending_story_open(window, cx);
        // main-only: mini apps open over the main window.
        self.run_pending_mini_app_action(window, cx);
        // main-only: the story tray, viewer and composer are the main
        // window's (`story_frame_upkeep`).
        self.story_frame_upkeep(cx);
        // main-only: the Masks strip's gate is the session's, wanted by
        // whichever window's editor is in Stickers mode.
        self.sync_editor_masks();
        // main-only: the story page is the main window's overlay.
        self.story_page_upkeep(window, cx);
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
        // The composer's Post and Save presses, each answered by its own
        // request (`story_composer_stage.rs`).
        self.story_composer_upkeep(cx);
        // The viewer's frame: it moves the story on and asks for the next
        // frame while something moves; a redraw that finds a held story due
        // again (the window active, a panel closed) starts the clock.
        self.story_viewer_upkeep(cx);
        // A cover picker opened on a clip still downloading reads it once
        // it is local (`story_albums/cover_strip.rs`).
        self.cover_upkeep(cx);
        // Phase 9.5: the viewer cover editor / privacy editor sent a
        // management call. Its own answer (matched by request id) closes
        // that story's panel on success or leaves it open showing the
        // error; a call let go without one just ends the wait. The call
        // outlives the panel and a step to another story.
        for call in self.stories.manage_calls.clone() {
            let Some(done) = self
                .session()
                .map(|session| session.stories.manage.outcome(call.request))
                .filter(|outcome| *outcome != StoryManageOutcome::InFlight)
            else {
                continue;
            };
            self.stories.end_manage_call(call.request);
            if done == StoryManageOutcome::Failed {
                self.show_story_manage_failure(call.request, Some(call.story));
            }
            if done == StoryManageOutcome::Saved {
                self.stories.with_viewer_ui(|open| open.close_manage(call));
            }
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
