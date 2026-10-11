//! Per-frame upkeep that `render` runs before building the window: things
//! that need `&mut Window` (inputs, notifications, deep links, story and
//! payment follow-ups) or keep timers ticking.

use super::*;
use quill::state::StoryPostOutcome;

impl QuillApp {
    pub(super) fn frame_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase 8.1: feed OS window focus into the notification decision, then
        // dispatch any notifications the reducer queued since the last frame.
        if let Some(live) = self.live.as_mut() {
            live.driver.session.app_active = window.is_window_active();
        }
        self.flush_notifications(window, cx);
        // Slice P1 fix-up: the checkout dialog opens on Buy press before
        // the form arrives — prefill the saved order info once, on the
        // first frame after the form answer lands. (This can't live in
        // `poll_live`: prefill needs a `&mut Window` for the inputs.)
        let payment_form = self.session().and_then(|s| s.payments.form.clone());
        if let (Some(dialog), Some(form)) = (self.payments.dialog.as_mut(), payment_form)
            && !dialog.prefilled
        {
            dialog.prefill_from_form(&form, window, cx);
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
            self.connection.status_note = "AI updated the draft".into();
        }
        if let Some((chat_id, rich, note)) = ai_blocks
            && open_chat == Some(chat_id)
        {
            let text = quill::rich::blocks_to_markup(&rich.blocks);
            self.composer.update(cx, |input, cx| {
                input.set_value(&text, window, cx);
            });
            self.connection.status_note = note.into();
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
        // Phase 9.1: resolve a tapped story whose `story` response landed
        // since the click (`getStory` prefetch finished).
        // Parity slice: prefill the folder editor once its `getChatFolder`
        // spec arrives.
        self.maybe_prefill_folder_editor(window, cx);
        if let Some((chat_id, story_id)) = self.chat_list.pending_story_open {
            let ready = self
                .session()
                .is_some_and(|s| s.stories.stories.contains_key(&(chat_id, story_id)));
            if ready {
                self.chat_list.pending_story_open = None;
                self.rebuild_story_viewer(ChatId(chat_id), story_id, cx);
            }
        }
        // `parity:platform-deep-links`: open the chat the deep link
        // resolved to (take-once; render owns the `Window`).
        if let Some((chat_id, action)) = self.links.pending_deep_link_open.take() {
            self.open_deep_link_chat(chat_id, &action, window, cx);
        }
        if let Some(ui) = self.links.pending_deep_link_ui.take() {
            self.run_deep_link_ui(ui, window, cx);
        }
        // A clicked mention, hashtag, command or link (`entity_links`).
        self.run_pending_link(window, cx);
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
                self.stories.cover_target = None;
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
            self.stories.viewer.close();
            self.stories.reaction_picker_open = false;
            self.stories.reply_open = false;
            self.connection.status_note = "Story deleted".into();
        }
        // Phase 4.6: push the playback clock into the seek slider entity so
        // the thumb follows elapsed time (the tick has no `&mut Window`).
        self.sync_seek_slider(window, cx);
        self.sync_signin(window, cx);
    }
}
