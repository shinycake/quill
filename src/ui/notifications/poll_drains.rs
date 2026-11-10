//! The poll's drains: answers that landed in the session and now show or
//! start something (a box, a viewer, a browser page, the clipboard, a status
//! note). The poll runs them only while Quill is unlocked; while locked they
//! stay where they landed and run at the first pass after the unlock
//! (docs/decisions/lock-is-a-render-state.md, "What runs while locked").
//! A step added here waits for the unlock by itself; one added to
//! `poll_live` outside its gate must be listed in scripts/lock-surfaces.txt
//! (`scripts/lock_surfaces.py`).
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::ChatId;

impl QuillApp {
    /// Drain what the last ingest left for the UI. Returns whether anything
    /// was drained (the poll redraws at once).
    pub(super) fn poll_drains(&mut self, cx: &mut Context<Self>) -> bool {
        let mut progressed = false;
        if let Some(result) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.messages.last_forward.take())
        {
            self.present_forward_answer(result, cx);
            progressed = true;
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.pump_psa_labels();
        }
        // M1: a `messageLink` response lands here (`getMessageLink`) —
        // copy the link to the clipboard, exactly like tdesktop's "Copy
        // Message Link".
        if let Some(link) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.messages.message_link_result.take())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(link));
            // Telegram Desktop `CopyPostLink`: a public link says so; a
            // private one warns that only members can open it.
            let public = self
                .live
                .as_ref()
                .is_some_and(|live| live.driver.session.messages.message_link_public);
            self.set_note(StatusNote::info(quill::message_menu::link_copied_note(
                public,
            )));
            progressed = true;
        }
        // Batch 7: keep a translated chat's translations coming, and
        // Saved Messages' sublists and tags loaded, in every window.
        if self.poll_every_window(cx) {
            progressed = true;
        }
        // `parity:platform-deep-links`: drive the launch-link flow —
        // `getDeepLinkInfo` once auth is Ready, then follow-ups / dialog /
        // deferred chat open from each terminal session state.
        self.pump_deep_link(cx);
        // M1 fix-up: a "Share link" gated off by
        // `messageProperties.can_get_link` (or a failed `getMessageLink`)
        // and a failed `resendMessages` surface here instead of silently
        // doing nothing.
        for err in self
            .live
            .as_mut()
            .map(|live| live.driver.session.messages.message_link_error.take_all())
            .unwrap_or_default()
        {
            self.set_note(StatusNote::error(err));
            progressed = true;
        }
        // Message widgets: their bots' answers and public data polls.
        let answers = self
            .live
            .as_mut()
            .map(|live| live.driver.take_widget_answers())
            .unwrap_or_default();
        if !answers.is_empty() {
            self.land_widget_answers(answers, cx);
            progressed = true;
        }
        // B7: a basic group became a supergroup: leave the old chat.
        if self.pump_chat_upgrades(cx) {
            progressed = true;
        }
        // B10: open a profile photo gallery that was waiting for its list.
        if self.pump_profile_gallery(cx) {
            progressed = true;
        }
        // Slice CL1: a refused chat-list action (`toggleChatIsPinned`,
        // `toggleChatIsMarkedAsUnread`, `deleteChatHistory`) surfaces
        // here instead of silently doing nothing.
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.chats_state.chat_action_error.take())
        {
            self.set_note(StatusNote::error(err));
            progressed = true;
        }
        // MED2 fix-up: a refused `recognizeSpeech` surfaces in the status
        // note instead of silently doing nothing after
        // "transcription requested".
        for err in self
            .live
            .as_mut()
            .map(|live| live.driver.session.messages.recognize_speech_error.take_all())
            .unwrap_or_default()
        {
            self.set_note(StatusNote::error(err));
            progressed = true;
        }
        // Slice msg-richtext-ai-tools: a failed AI request surfaces in
        // the status note instead of silently doing nothing after "AI
        // working…".
        for err in self
            .live
            .as_mut()
            .map(|live| live.driver.session.messages.ai_error.take_all())
            .unwrap_or_default()
        {
            self.set_note(StatusNote::error(err));
            progressed = true;
        }
        for err in self
            .live
            .as_mut()
            .map(|live| live.driver.session.messages.resend_error.take_all())
            .unwrap_or_default()
        {
            self.set_note(StatusNote::error(err));
            progressed = true;
        }
        // TDLib 1.8.68 community management (rename / photo /
        // permissions / delete) refused.
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.groups.community_error.take())
        {
            self.set_note(StatusNote::error(err));
            progressed = true;
        }
        // Slice G1 fix-up: an invite-link mutation (create/edit/revoke/
        // replace-primary) failed — the loaded list is kept, so the
        // error surfaces here instead of wiping the panel.
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.groups.invite_link_error.take())
        {
            self.set_note(StatusNote::error(err));
            progressed = true;
        }
        // Ban / delete-all / report-spam from the delete box, and "Save to
        // Profile": their outcome lands in the status note.
        for note in self
            .live
            .as_mut()
            .map(|live| live.driver.session.messages.message_action_note.take_all())
            .unwrap_or_default()
        {
            self.set_note(StatusNote::info(note));
            progressed = true;
        }
        // The transfer-ownership dialog: a finished transfer closes it
        // (and leaves, for "appoint and leave").
        if self.finish_ownership_transfer_ui(cx) {
            progressed = true;
        }
        // The sticker set box: an edit's toast, refresh, or the set's end.
        if self.sticker_set_box_tick(cx) {
            progressed = true;
        }
        // Slice CL3: a `reportChat` outcome arrived — surface it in the
        // status bar alongside the other async error drains.
        if let Some(note) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.chats_state.report_chat_outcome.take())
        {
            self.set_note(StatusNote::info(note));
            progressed = true;
        }
        // Slice G1 fix-up: `updateChatMember` dropped the member-list
        // caches — refetch the open dialog's page so it shows the new
        // membership instead of sticking on "Loading members…".
        let stale_chats: Vec<i64> = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.groups.member_list_stale))
            .unwrap_or_default();
        if self
            .admin
            .member_dialog
            .as_ref()
            .is_some_and(|dialog| stale_chats.contains(&dialog.chat_id.0))
        {
            self.refresh_member_dialog(cx);
            progressed = true;
        }
        self.inline_members_stale(&stale_chats, cx);
        // The Administrators list of a stale chat asks again: it is fetched
        // only on open, and would otherwise draw "Loading" forever.
        if !stale_chats.is_empty()
            && let Some(live) = self.live.as_mut()
        {
            for chat in &stale_chats {
                if !live.driver.session.groups.admin_lists.contains_key(chat)
                    && let Ok(Some(_)) = live.driver.fetch_chat_administrators(ChatId(*chat))
                {
                    progressed = true;
                }
            }
        }
        // Phase 3.2: bot answers to inline keyboard callback presses.
        if let Some(quill::state::CallbackAnswer { answer, chat }) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.bots.last_callback_answer.take())
        {
            // A URL in it opens as a link clicked where the button was.
            let asker = chat.and_then(|chat| self.window_showing(chat));
            self.as_asked_by(asker, |this| this.present_callback_answer(answer, cx));
            progressed = true;
        }
        // B1: `getLoginUrlInfo` / `getLoginUrl` answers to login-URL button
        // presses. The button's request context (kept in the session while
        // the request was in flight) backs the confirmation dialog and the
        // error degrade.
        let login_url = self.live.as_mut().and_then(|live| {
            let info = live.driver.session.bots.last_login_url_info.take()?;
            let request = live.driver.session.bots.login_url_request.take();
            Some((info, request))
        });
        if let Some((info, request)) = login_url {
            // Its consent box and its link belong to the window showing
            // the button's chat, not to the main window this poll runs in.
            let asker = request
                .as_ref()
                .and_then(|request| self.window_showing(request.chat_id));
            self.as_asked_by(asker, |this| this.present_login_url_info(info, request, cx));
            progressed = true;
        }
        // Slice P1: a `paymentResult` verification URL — the provider's
        // page the buyer must complete (schema:4740).
        if let Some(step) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.payments.verification_url.take())
        {
            self.open_payment_verification(&step, cx);
            progressed = true;
        }
        // Slice P1 fix-up: a failed `getPaymentReceipt` surfaces in the
        // status bar — the receipt dialog never opens, and `payment_note`
        // only renders inside the checkout dialog.
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.payments.receipt_error.take())
        {
            self.set_note(StatusNote::error(err));
            progressed = true;
        }
        if let Some(note) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.payments.unlocked_notice.take())
        {
            self.set_note(StatusNote::info(note));
            progressed = true;
        }
        for notice in self
            .live
            .as_mut()
            .map(|live| live.driver.session.messages.send_permission_error.take_all())
            .unwrap_or_default()
        {
            self.set_note(StatusNote::error(notice));
            progressed = true;
        }
        for notice in self
            .live
            .as_mut()
            .map(|live| live.driver.session.messages.flood_notice.take_all())
            .unwrap_or_default()
        {
            self.set_note(StatusNote::error(notice));
            progressed = true;
        }
        // TDLib refused the `logOut`: the account stays signed in.
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.auth_state.logout_refused.take())
        {
            self.set_note(StatusNote::error(crate::ui::user_error::user_error_for(
                quill::lang::keys::Q_NOTES_FAILED_LOG_OUT,
                &err,
            )));
            progressed = true;
        }
        self.poll_folder_new_chats();
        self.prune_drop_topic();
        if self.drain_folder_share(cx)
            | self.drive_folder_invite(cx)
            | self.drain_folder_limit(cx)
            | self.drain_chat_look(cx)
            | self.drain_wallpaper_preview(cx)
            | self.drain_earn(cx)
            | self.drain_language(cx)
            | self.drain_peer_qr(cx)
            | self.drain_story_stealth(cx)
            | self.drain_communities(cx)
        {
            progressed = true;
        }
        progressed
    }
}
