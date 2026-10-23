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
        // Phase 8.1: feed OS window focus into the notification decision, then
        // dispatch any notifications the reducer queued since the last frame.
        if let Some(live) = self.live.as_mut() {
            live.driver.session.app_active = window.is_window_active();
        }
        self.flush_notifications(window, cx);
        self.sync_history_holds();
        self.fetch_missing_reply_target();
        // The inline results button's round trip back to its chat.
        self.flush_inline_return(window, cx);
        // A topic or thread switch without a window at hand asked for the
        // new slot's draft to be restored into the composer.
        self.restore_draft_if_pending(window, cx);
        self.adopt_remote_draft(window, cx);
        self.sync_composer_placeholder(window, cx);
        // The chat windows open at the last run come back once the
        // account is ready (`chat_window::saved`).
        self.restore_chat_windows(window, cx);
        // A video chosen for an owned set finished downloading: its editor.
        self.sticker_box_frame_upkeep(window, cx);
        // Revenue and affiliate pages: the amount field, the setup
        // sliders, program photos and pages, profile balances.
        self.earn_frame_upkeep(window, cx);
        // Gift market: an auction whose state arrived, the followed
        // auction, toasts and sticker downloads.
        self.gift_market_frame_upkeep(window, cx);
        // "Stream with...": the server error of a failed request.
        self.rtmp_start_frame_upkeep(window, cx);
        // Starting a video chat: the "join as" list, a refused create.
        self.group_call_create_frame_upkeep(window, cx);
        // Gift transfer: Fragment's export page, the transferred toast.
        self.gift_transfer_frame_upkeep(window, cx);
        // Grams: Fragment's withdrawal page, the password field.
        self.grams_withdraw_frame_upkeep(window, cx);
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
        // Slice msg-richtext-ai-tools: an AI answer for the open chat's
        // composer replaces the draft (this needs `&mut Window` for the
        // input, so it can't live in `poll_live`). A late answer for a
        // chat the user has since left is dropped, never applied blindly.
        // Rich answers are written back as editor markup
        // (`blocks_to_markup`, the inverse of `markup_to_blocks`) so the
        // rich send path rebuilds headings, lists, details, and dividers.
        let ai_text = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.messages.ai_composer_text.take());
        let ai_blocks = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.messages.ai_composer_blocks.take());
        let open_chat = self
            .live
            .as_ref()
            .and_then(|live| live.driver.session.open_chat);
        if let Some((chat_id, text)) = ai_text
            && open_chat == Some(chat_id)
        {
            self.set_composer_markup(&text, window, cx);
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_REST_AI_UPDATED_THE_DRAFT)));
        }
        if let Some((chat_id, rich, note)) = ai_blocks
            && open_chat == Some(chat_id)
        {
            let text = quill::rich::blocks_to_markup(&rich.blocks);
            self.composer.update(cx, |input, cx| {
                input.set_value(&text, window, cx);
            });
            self.connection.set_note(StatusNote::info(note));
        }
        // Phase A1: keep the slow-mode countdown ticking while the open
        // chat is gated (spawns at most one 1s task per open chat).
        self.ensure_slow_mode_tick(cx);
        // Phase B3: keep self-destruct countdown badges fresh while the
        // open chat has a live `self_destruct_in` timer (same 1s task
        // pattern as slow mode).
        self.ensure_self_destruct_tick(cx);
        // Live-location countdowns: refreshed at the pace their labels
        // change, only while the open chat shows a running one.
        self.ensure_live_location_tick(cx);
        // Phase C1: keep the call overlay's ringing / connected clock
        // fresh while a call is tracked (same 1s task pattern).
        self.ensure_call_tick(cx);
        // The call window and its sounds follow the call.
        self.sync_call_window(cx);
        self.sync_group_call_window(cx);
        self.sync_call_sounds();
        // MED4b: debounced `getLinkPreview` prefetch for the
        // detected-URL chip (spawns at most one timer per new URL).
        self.maybe_prefetch_link_preview(cx);
        // Parity slice: prefill the folder editor once its `getChatFolder`
        // spec arrives.
        self.maybe_prefill_folder_editor(window, cx);
        self.resolve_pending_story_open(window, cx);
        // `parity:platform-deep-links`: open the chat the deep link
        // resolved to (take-once; render owns the `Window`).
        if let Some((chat_id, action)) = self.links.pending_deep_link_open.take() {
            self.open_deep_link_chat(chat_id, &action, window, cx);
            self.drop_shared_card(cx);
        }
        if let Some(ui) = self.links.pending_deep_link_ui.take() {
            self.run_deep_link_ui(ui, window, cx);
        }
        // A clicked mention, hashtag, command or link (`entity_links`).
        self.run_pending_link(window, cx);
        self.pump_collectible_info(cx);
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
        if self.stories.cover_sent && self.session().is_some_and(|s| !s.stories.manage.pending) {
            self.stories.cover_sent = false;
            if self
                .session()
                .is_some_and(|s| s.stories.manage.error.is_none())
            {
                self.stories.cover = None;
            }
        }
        if self.stories.privacy_sent && self.session().is_some_and(|s| !s.stories.manage.pending) {
            self.stories.privacy_sent = false;
            if self
                .session()
                .is_some_and(|s| s.stories.manage.error.is_none())
            {
                self.stories.privacy_edit = None;
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
            self.release_story_viewer();
            self.stories.reaction_picker_open = false;
            self.stories.reply_open = false;
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
