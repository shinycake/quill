//! Per-frame upkeep that `render` runs before building the window: things
//! that need `&mut Window` (inputs, notifications, deep links, story and
//! payment follow-ups) or keep timers ticking.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};
use quill::state::StoryPostOutcome;

impl QuillApp {
    pub(super) fn frame_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(feature = "frame-profiler")]
        self.frame_profile_tick(window);
        // main-only: notification clicks, other accounts and delivery are the
        // app's; a chat they open goes through the router (`route_chat_open`).
        self.flush_notifications(window, cx);
        // What every window's frame runs, here with the main window's
        // state in place (`window_upkeep`).
        self.per_window_upkeep(window, cx);
        // The chat windows open at the last run come back once the
        // account is ready (`chat_window::saved`).
        // main-only: it opens the chat windows themselves.
        self.restore_chat_windows(window, cx);
        // A video chosen for an owned set finished downloading: its editor.
        // main-only: the pick carries its window and the editor opens there
        // (`hand_to_chat_window`); the main frame only notices the file.
        self.sticker_box_frame_upkeep(window, cx);
        // Revenue and affiliate pages: the amount field, the setup
        // sliders, program photos and pages, profile balances.
        self.earn_frame_upkeep(window, cx);
        // "Stream with...": the server error of a failed request.
        self.rtmp_start_frame_upkeep(window, cx);
        // Starting a video chat: the "join as" list, a refused create.
        self.group_call_create_frame_upkeep(window, cx);
        // Slice P1 fix-up: the checkout dialog opens on Buy press before
        // the form arrives — prefill the saved order info once, on the
        // first frame after the form answer lands. (This can't live in
        // `poll_live`: prefill needs a `&mut Window` for the inputs.)
        if self.payments.dialog.as_ref().is_some_and(|d| !d.prefilled) {
            let payment_form = self.session().and_then(|s| s.payments.form.clone());
            if let (Some(dialog), Some(form)) = (self.payments.dialog.as_mut(), payment_form) {
                dialog.prefill_from_form(&form, window, cx);
            }
        }
        // main-only: the AI compose box carries the window it was opened in.
        self.ai_compose_frame_upkeep(window, cx);
        self.premium_limit_toast_upkeep(window, cx);
        // MED4b: debounced `getLinkPreview` prefetch for the
        // detected-URL chip (spawns at most one timer per new URL).
        // main-only: the session keeps one composer preview; every window
        // asking would refetch it for each window in turn.
        self.maybe_prefetch_link_preview(cx);
        // Parity slice: prefill the folder editor once its `getChatFolder`
        // spec arrives.
        self.maybe_prefill_folder_editor(window, cx);
        // main-only: stories open in the main window.
        self.resolve_pending_story_open(window, cx);
        // `parity:platform-deep-links`: open the chat or box the deep link
        // resolved to (take-once; render owns the `Window`).
        // main-only: deep links arrive in the main window; the chat they
        // name goes through the router.
        self.run_resolved_deep_link(window, cx);
        self.pump_collectible_info(cx);
        self.pump_promo_request(cx);
        // main-only: mini apps open over the main window.
        self.run_pending_mini_app_action(window, cx);
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
        // management call — once `story_manage.pending` clears, close
        // the panel on success or leave it open showing the error.
        if self.stories.viewer_ui().cover_sent
            && self.session().is_some_and(|s| !s.stories.manage.pending)
        {
            let saved = self
                .session()
                .is_some_and(|s| s.stories.manage.error.is_none());
            self.stories.with_viewer_ui(|open| {
                open.cover_sent = false;
                if saved {
                    open.cover = None;
                }
            });
        }
        if self.stories.viewer_ui().privacy_sent
            && self.session().is_some_and(|s| !s.stories.manage.pending)
        {
            let saved = self
                .session()
                .is_some_and(|s| s.stories.manage.error.is_none());
            self.stories.with_viewer_ui(|open| {
                open.privacy_sent = false;
                if saved {
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_REST_STORY_DELETED)));
        }
        self.follow_story_tray(cx);
        self.story_viewers_scrolled(cx);
        // Phase 4.6: push the playback clock into the seek slider entity so
        // the thumb follows elapsed time (the tick has no `&mut Window`).
        self.sync_seek_slider(window, cx);
        self.sync_signin(window, cx);
    }
}
