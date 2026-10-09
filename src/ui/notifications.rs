//! Poll loop and OS notification threads.

use super::app::QuillApp;
use super::audio::{NotificationSound, notification_sound};
use super::connect_ui::live_status_for;
use super::notification_settings::MAX_OS_NOTIFICATION_THREADS;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::notify::{NotificationAction, NotificationSoundKind, QueuedNotification};
use quill::state::RedrawNeed;
use quill::telegram::envelope::{AuthorizationState, ChatNotificationSettings};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;
pub(super) fn notification_settings_json(settings: &ChatNotificationSettings) -> String {
    format!(
        r#"{{"@type":"chatNotificationSettings","use_default_mute_for":{},"mute_for":{},"use_default_sound":{},"sound_id":"{}","use_default_show_preview":{},"show_preview":{},"use_default_mute_stories":{},"mute_stories":{},"use_default_story_sound":{},"story_sound_id":"{}","use_default_show_story_poster":{},"show_story_poster":{},"use_default_disable_pinned_message_notifications":{},"disable_pinned_message_notifications":{},"use_default_disable_mention_notifications":{},"disable_mention_notifications":{}}}"#,
        settings.use_default_mute_for,
        settings.mute_for,
        settings.use_default_sound,
        settings.sound_id,
        settings.use_default_show_preview,
        settings.show_preview,
        settings.use_default_mute_stories,
        settings.mute_stories,
        settings.use_default_story_sound,
        settings.story_sound_id,
        settings.use_default_show_story_poster,
        settings.show_story_poster,
        settings.use_default_disable_pinned_message_notifications,
        settings.disable_pinned_message_notifications,
        settings.use_default_disable_mention_notifications,
        settings.disable_mention_notifications
    )
}

/// Main-thread time one poll tick may spend applying TDLib updates (half a
/// 60 Hz frame).
const INGEST_BUDGET: Duration = Duration::from_millis(8);

/// How often urgent TDLib updates may redraw the window while it is
/// inactive.
const INACTIVE_REDRAW: Duration = Duration::from_millis(500);

/// Batched redraws for updates that only touch the chat list
/// ([`RedrawNeed::ChatList`]), window active / inactive: a typing line or
/// an online dot in another chat may show this late.
const CHAT_LIST_REDRAW: (Duration, Duration) =
    (Duration::from_millis(400), Duration::from_millis(2000));

/// Batched full redraws for updates nobody waits on
/// ([`RedrawNeed::Later`]: automatic downloads, user records).
const LATER_REDRAW: (Duration, Duration) =
    (Duration::from_millis(1000), Duration::from_millis(4000));

/// What the TDLib poll has asked to redraw and not drawn yet.
///
/// Urgent updates ([`RedrawNeed::Now`]) redraw the whole window at once
/// (at most every `INACTIVE_REDRAW` behind another app). The rest are
/// batched: chat-list-only changes redraw just the chat list
/// (`notify_chat_list`) at most once per `CHAT_LIST_REDRAW`, others a full
/// redraw at most once per `LATER_REDRAW`. The two batches are tracked
/// apart, so a pending full redraw never holds a chat-list change back
/// past its own interval. A held-back redraw is never dropped: the poll
/// loop calls back at least every 120 ms, with [`RedrawNeed::Nothing`]
/// when nothing arrived.
#[derive(Debug)]
pub(super) struct PolledRedraw {
    /// The last full redraw the poll asked for.
    last_full: std::time::Instant,
    /// The last chat-list redraw (or full redraw, which includes it).
    last_list: std::time::Instant,
    /// The most urgent full redraw not drawn yet (`Nothing`, `Later` or
    /// `Now`).
    full: RedrawNeed,
    /// A chat-list redraw is owed.
    list: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PolledAction {
    Wait,
    ChatList,
    Full,
}

impl PolledRedraw {
    pub(super) fn new(now: std::time::Instant) -> Self {
        Self {
            last_full: now,
            last_list: now,
            full: RedrawNeed::Nothing,
            list: false,
        }
    }

    /// Record `need` and say what to redraw now.
    pub(super) fn decide(
        &mut self,
        need: RedrawNeed,
        active: bool,
        now: std::time::Instant,
    ) -> PolledAction {
        match need {
            RedrawNeed::Nothing => {}
            RedrawNeed::ChatList => self.list = true,
            RedrawNeed::Later | RedrawNeed::Now => self.full = self.full.max(need),
        }
        let pick = |(active_gap, inactive_gap): (Duration, Duration)| {
            if active { active_gap } else { inactive_gap }
        };
        let full_due = match self.full {
            RedrawNeed::Now => active || now.duration_since(self.last_full) >= INACTIVE_REDRAW,
            RedrawNeed::Later => now.duration_since(self.last_full) >= pick(LATER_REDRAW),
            RedrawNeed::Nothing | RedrawNeed::ChatList => false,
        };
        if full_due {
            return self.drawn(now);
        }
        if self.list && now.duration_since(self.last_list) >= pick(CHAT_LIST_REDRAW) {
            self.list = false;
            self.last_list = now;
            return PolledAction::ChatList;
        }
        PolledAction::Wait
    }

    /// A full redraw now, whatever is pending and however recently the
    /// window drew: for what only a render hands out (desktop
    /// notifications, their sounds, forced replies), which must not wait
    /// for the inactive window's interval.
    pub(super) fn drawn(&mut self, now: std::time::Instant) -> PolledAction {
        self.full = RedrawNeed::Nothing;
        self.list = false;
        self.last_full = now;
        self.last_list = now;
        PolledAction::Full
    }
}

impl QuillApp {
    pub(super) fn spawn_poll_loop(&mut self, cx: &mut Context<Self>) {
        let generation = self.connection_generation;
        cx.spawn(async move |this, cx| {
            // Adaptive cadence: drain quickly while updates are flowing,
            // back off when idle so a quiet app doesn't wake 25×/s.
            const BUSY: Duration = Duration::from_millis(10);
            const IDLE_MAX: Duration = Duration::from_millis(120);
            let mut delay = Duration::from_millis(40);
            loop {
                cx.background_executor().timer(delay).await;
                let step = this
                    .update(cx, |this, cx| {
                        if generation != this.connection_generation {
                            return None;
                        }
                        let busy = this.poll_live(cx);
                        this.live.is_some().then_some(busy)
                    })
                    .ok()
                    .flatten();
                match step {
                    None => break,
                    Some(true) => delay = BUSY,
                    Some(false) => delay = (delay * 2).clamp(BUSY, IDLE_MAX),
                }
            }
        })
        .detach();
    }

    pub(super) fn reload_account_keybindings(&mut self, cx: &mut Context<Self>) {
        super::keybindings::invalidate_account_keybindings(
            &mut self.keybindings_applied,
            &mut self.keybinding_capture,
            &mut self.keybinding_error,
        );
        self.apply_pending_keybindings(cx);
    }

    fn apply_pending_keybindings(&mut self, cx: &mut Context<Self>) {
        if self.keybindings_applied {
            return;
        }
        let Some(live) = self.live.as_ref() else {
            return;
        };
        let customs = live.driver.load_custom_keybindings();
        // Empty prefs rebuild defaults too, replacing the previous account's chords.
        super::keybindings::apply_custom_bindings(cx, &customs);
        self.keybindings_applied = true;
    }

    /// Redraw for what the TDLib poll applied, as urgently as it needs
    /// (`quill::state::redraw_need`; batching rules on [`PolledRedraw`]).
    /// Called after every poll, with [`RedrawNeed::Nothing`] when nothing
    /// arrived, so held-back redraws still land.
    /// `deliver`: something only a render hands out is queued (see
    /// [`PolledRedraw::drawn`]); redraw at once even behind another app.
    pub(super) fn redraw_polled(
        &mut self,
        need: RedrawNeed,
        deliver: bool,
        cx: &mut Context<Self>,
    ) {
        let active = self.window_active.get();
        let now = std::time::Instant::now();
        let action = if deliver {
            self.polled_redraw.drawn(now)
        } else {
            self.polled_redraw.decide(need, active, now)
        };
        match action {
            PolledAction::Wait => {}
            PolledAction::ChatList => self.notify_chat_list(cx),
            PolledAction::Full => cx.notify(),
        }
    }

    /// The app state `poll_live` itself sets, outside the session, that
    /// the window draws (status line, login-review box, folder tab, lost
    /// connection): compared before and after a poll.
    fn polled_chrome(&self) -> (String, bool, Option<i32>, bool) {
        (
            self.status_note.clone(),
            self.login_prevented.is_some(),
            self.folder_tab,
            self.connection_lost,
        )
    }

    /// Drain and apply everything TDLib has queued. Returns whether
    /// anything arrived, so the poll loop can stay fast during bursts and
    /// back off while idle.
    pub(super) fn poll_live(&mut self, cx: &mut Context<Self>) -> bool {
        // What this poll may change outside the session (the status line,
        // the login-review box, the folder tab): any change redraws at once.
        let shown_before = self.polled_chrome();
        self.poll_device_qr(cx);
        self.sync_presence();
        self.drain_account_notices();
        self.apply_pending_keybindings(cx);
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        let prev_auth = live.driver.session.auth.clone();
        let mut progressed = live
            .driver
            .session
            .expire_pending_bot_messages(quill::state::unix_ms_now());
        let mut send_failed = false;
        // How urgently what arrives needs a redraw; a bot message that
        // expired above shows in the open chat.
        let mut need = if progressed {
            RedrawNeed::Now
        } else {
            RedrawNeed::Nothing
        };
        // Slice auth-logout-warning D1 fix-up: latch `LoggingOut` inside
        // the drain loop. TDLib can queue both `LoggingOut` and `Closed`
        // before one poll runs; a `prev_auth`-only check then sees
        // Ready → Closed and misses the restart entirely.
        let mut saw_logging_out = matches!(prev_auth, AuthorizationState::LoggingOut);
        // Apply updates under a per-tick time budget: a sync burst (thousands
        // of updates after reconnecting) is spread over several frames
        // instead of freezing one. Leftovers stay queued and the loop comes
        // back at its busy cadence.
        let budget_start = std::time::Instant::now();
        let mut budget_hit = false;
        while let Some(owned) = live.bridge.next_timeout(Duration::from_millis(0)) {
            if super::frame_clock::trace_notify() {
                let payload = format!("{:?}", owned.envelope.payload);
                let name = payload
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or_default();
                super::frame_clock::trace_ingested(name);
            }
            need = need.max(quill::state::redraw_need(
                &live.driver.session,
                &owned.envelope,
            ));
            if live.driver.ingest(owned).is_err() {
                send_failed = true;
            }
            saw_logging_out = saw_logging_out
                || matches!(live.driver.session.auth, AuthorizationState::LoggingOut);
            progressed = true;
            if budget_start.elapsed() >= INGEST_BUDGET {
                budget_hit = true;
                break;
            }
        }
        // The receive thread ended on its own (panic or closed channel):
        // nothing more will arrive, so surface it like an unexpected
        // Closed with a Retry instead of a UI that silently goes stale.
        if bridge_lost(
            live.bridge.stopped_unexpectedly(),
            budget_hit,
            self.connection_lost,
        ) {
            self.connection_lost = true;
            self.status_note = "Connection to Telegram was closed".into();
            progressed = true;
            need = RedrawNeed::Now;
        }
        // `parity:proxy-settings`: first `getProxies` + auto-switch.
        if live.driver.proxy_tick(quill::state::unix_ms_now()) {
            progressed = true;
            need = RedrawNeed::Now;
        }
        // A desktop notification, its sound or a forced reply is handed
        // out by the next render (`flush_notifications`): draw it now,
        // whatever the updates were and whether or not the window is in
        // front.
        let deliver = !live.driver.session.pending_notifications.is_empty()
            || live.driver.session.pending_force_reply.is_some()
            || !live.driver.session.pending_sound_plays.is_empty();
        // Parity slice: the selected folder tab may have been deleted or
        // removed remotely (`updateChatFolders`); fall back to Main.
        if let Some(folder_id) = self.folder_tab
            && !live
                .driver
                .session
                .chat_folders
                .iter()
                .any(|f| f.id == folder_id)
        {
            self.folder_tab = None;
        }
        let err = live.driver.session.last_auth_error;
        let new_auth = live.driver.session.auth.clone();
        if !matches!(&new_auth, AuthorizationState::WaitRegistration { terms: Some(terms) } if self.accepted_registration_terms.as_ref() == Some(terms))
        {
            self.accepted_registration_terms = None;
        }
        // Slice auth-logout-warning: the `logOut` flow ends in Closed —
        // restart the live connection at the end of this poll so the user
        // lands back on the login screen instead of the dead "Closed"
        // view. The quit path (`close`) also ends in Closed but never
        // passes through LoggingOut, so it never restarts.
        let logged_out = logout_restart_trigger(saw_logging_out, &new_auth);
        // Slice A10: recovery-code entry only makes sense in WaitPassword.
        if !matches!(new_auth, AuthorizationState::WaitPassword { .. }) {
            self.recovery_mode = false;
        }
        if send_failed {
            self.status_note = "failed to send TDLib request".into();
        } else if progressed {
            if let Some(err) = err {
                self.status_note = err.user_message();
            } else if new_auth != prev_auth {
                self.status_note = live_status_for(&new_auth);
            }
        }
        // From here on, `progressed` marks results the UI drains (status
        // notes, finished exports, links): those redraw at once.
        let ingested = std::mem::take(&mut progressed);
        if let Some(result) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.last_forward.take())
        {
            self.present_forward_result(result, cx);
            progressed = true;
        }
        // `parity:platform-chat-export` — drive export paging and surface
        // the result (file path or error) as a status note, then clear it.
        if let Some(live) = self.live.as_mut() {
            live.driver.pump_chat_export();
            live.driver.pump_account_export();
            if live
                .driver
                .session
                .account_export
                .as_ref()
                .is_some_and(|e| !e.finished.load(std::sync::atomic::Ordering::Acquire))
            {
                progressed = true;
            }
            let note = live
                .driver
                .session
                .chat_export
                .as_ref()
                .filter(|export| export.settled())
                .map(|export| {
                    if let Some(path) = export.finished_path.as_ref() {
                        format!(
                            "Exported {} messages to {}",
                            export.messages.len(),
                            path.display()
                        )
                    } else {
                        format!(
                            "Chat export failed: {}",
                            export.failed.as_deref().unwrap_or("unknown error")
                        )
                    }
                });
            if let Some(note) = note {
                live.driver.session.chat_export = None;
                self.status_note = note;
                progressed = true;
            }
        }
        // M1: a `messageLink` response lands here (`getMessageLink`) —
        // copy the link to the clipboard, exactly like tdesktop's "Copy
        // Message Link".
        if let Some(link) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.message_link_result.take())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(link));
            // Telegram Desktop `CopyPostLink`: a public link says so; a
            // private one warns that only members can open it.
            let public = self
                .live
                .as_ref()
                .is_some_and(|live| live.driver.session.message_link_public);
            self.status_note = if public {
                "Link copied to clipboard."
            } else {
                "This link will only work for members of this chat."
            }
            .into();
            progressed = true;
        }
        // Batch 7: keep a translated chat's translations coming.
        if self.pump_translation() {
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
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.message_link_error.take())
        {
            self.status_note = err;
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
            .and_then(|live| live.driver.session.chat_action_error.take())
        {
            self.status_note = err;
            progressed = true;
        }
        // MED2 fix-up: a refused `recognizeSpeech` surfaces in the status
        // note instead of silently doing nothing after
        // "transcription requested".
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.recognize_speech_error.take())
        {
            self.status_note = err;
            progressed = true;
        }
        // Slice msg-richtext-ai-tools: a failed AI request surfaces in
        // the status note instead of silently doing nothing after "AI
        // working…".
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.ai_error.take())
        {
            self.status_note = err;
            progressed = true;
        }
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.resend_error.take())
        {
            self.status_note = err;
            progressed = true;
        }
        // Slice G1 fix-up: an invite-link mutation (create/edit/revoke/
        // replace-primary) failed — the loaded list is kept, so the
        // error surfaces here instead of wiping the panel.
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.invite_link_error.take())
        {
            self.status_note = err;
            progressed = true;
        }
        // Ban / delete-all / report-spam from the delete box, and "Save to
        // Profile": their outcome lands in the status note.
        if let Some(note) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.message_action_note.take())
        {
            self.status_note = note;
            progressed = true;
        }
        // Slice CL3: a `reportChat` outcome arrived — surface it in the
        // status bar alongside the other async error drains.
        if let Some(note) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.report_chat_outcome.take())
        {
            self.status_note = note;
            progressed = true;
        }
        // Slice G1 fix-up: `updateChatMember` dropped the member-list
        // caches — refetch the open dialog's page so it shows the new
        // membership instead of sticking on "Loading members…".
        let stale_chats: Vec<i64> = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.member_list_stale))
            .unwrap_or_default();
        if self
            .member_dialog
            .as_ref()
            .is_some_and(|dialog| stale_chats.contains(&dialog.chat_id.0))
        {
            self.refresh_member_dialog(cx);
            progressed = true;
        }
        // Phase 3.2: bot answers to inline keyboard callback presses.
        if let Some(answer) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.last_callback_answer.take())
        {
            self.present_callback_answer(answer, cx);
            progressed = true;
        }
        // B1: `getLoginUrlInfo` / `getLoginUrl` answers to login-URL button
        // presses. The button's request context (kept in the session while
        // the request was in flight) backs the confirmation dialog and the
        // error degrade.
        let login_url = self.live.as_mut().and_then(|live| {
            let info = live.driver.session.last_login_url_info.take()?;
            let request = live.driver.session.login_url_request.take();
            Some((info, request))
        });
        if let Some((info, request)) = login_url {
            self.present_login_url_info(info, request, cx);
            progressed = true;
        }
        // Slice P1: a `paymentResult` verification URL — the provider's
        // page the buyer must complete (schema:4740). Open it in the OS
        // browser; the success/failure note is already on the session.
        if let Some(url) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.payment_verification_url.take())
        {
            self.open_message_url(&url, cx);
            progressed = true;
        }
        // Slice P1 fix-up: a failed `getPaymentReceipt` surfaces in the
        // status bar — the receipt dialog never opens, and `payment_note`
        // only renders inside the checkout dialog.
        if let Some(err) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.payment_receipt_error.take())
        {
            self.status_note = err;
            progressed = true;
        }
        if let Some(notice) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.send_permission_error.take())
        {
            self.status_note = notice;
            progressed = true;
        }
        self.finish_successful_sends(cx);
        if progressed || send_failed || self.polled_chrome() != shown_before {
            need = RedrawNeed::Now;
        }
        progressed |= ingested;
        // Bots slice: re-run the inline progress check after every batch
        // of session updates — arm the debounced dispatch for the current
        // trigger, if any. Idempotent: no-ops when the slot is already
        // fresh or a timer is armed.
        if progressed {
            self.progress_inline_mode(cx);
        }
        self.redraw_polled(need, deliver, cx);
        self.discard_stopped_media_playback(cx);
        self.resume_pending_gif(cx);
        self.resume_pending_video(cx);
        self.resume_pending_audio(cx);
        self.resume_pending_voice(cx);
        // Parity slice 5: a viewer video whose clip just finished downloading.
        self.resume_pending_viewer_video(cx);
        if logged_out {
            self.restart_live_connection(cx);
        }
        progressed || send_failed
    }

    /// Slice auth-logout-warning: drop the logged-out client and start a
    /// fresh connection — the auth flow resumes at
    /// `WaitTdlibParameters` → phone entry, i.e. the login screen. The
    /// old client is already Closed, so `Drop` joins its receive thread
    /// immediately instead of waiting out the 5s close timeout. If the
    /// restart fails the status line says so and the poll loop (which
    /// breaks on `live.is_none()`) stops.
    pub(super) fn restart_live_connection(&mut self, cx: &mut Context<Self>) {
        self.connection_lost = false;
        self.marketplace_open = false;
        self.marketplace_error = None;
        self.marketplace_private = true;
        let old = self.live.take();
        drop(old);
        // D2 fix-up: the warning promises "Downloaded media will be
        // erased from this device" — TDLib destroyed its own data, but
        // Quill's decrypted media scratch is only swept at startup.
        quill::local_path::sweep_media_caches();
        self.start_connection(cx);
    }

    /// Phase 8.1: drain notification click callbacks (focus the chat) and
    /// dispatch newly queued notifications on worker threads. Runs from
    /// `render`, which is the only UI path with a `&mut Window`.
    pub(super) fn flush_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A resolved comment thread moves the view into its discussion group.
        self.advance_thread(window, cx);
        let clicks: Vec<(ChatId, NotificationAction)> = self
            .notify_clicks
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default();
        for (chat_id, action) in clicks {
            self.run_notification_action(chat_id, action, window, cx);
        }
        // Notifications whose chat was read elsewhere (or removed by TDLib)
        // are withdrawn from the OS notification center.
        let clears: Vec<ChatId> = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.pending_notification_clears))
            .unwrap_or_default();
        for chat_id in clears {
            self.dismiss_os_notification(chat_id, cx);
        }
        // B1: force-reply — an incoming message demanded a reply; drain
        // from the live or demo session and arm the composer.
        let force_target = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.pending_force_reply.take())
            .or_else(|| {
                self.demo_session
                    .as_mut()
                    .and_then(|session| session.pending_force_reply.take())
            });
        if let Some(target) = force_target {
            self.drain_force_reply(target, window, cx);
        }
        let queued: Vec<QueuedNotification> = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.pending_notifications))
            .unwrap_or_default();
        for queued in queued {
            if let Some(kind) = queued.sound {
                self.play_notification_sound(kind);
            }
            self.spawn_os_notification(queued, cx);
        }
        // Parity slice: custom sounds whose downloads just completed.
        let plays: Vec<PathBuf> = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.pending_sound_plays))
            .unwrap_or_default();
        for path in plays {
            self.notification_sounds.play(NotificationSound::File(path));
        }
    }

    /// A notification was clicked or one of its buttons pressed. "Mark as
    /// read" works in the background (the window stays where it is); "Open"
    /// and "Reply" bring the app forward with the chat selected, and Reply
    /// also focuses the composer (GPUI notifications have no inline text
    /// field, see `quill::notify::NotificationAction`).
    pub(super) fn run_notification_action(
        &mut self,
        chat_id: ChatId,
        action: NotificationAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action == NotificationAction::MarkRead {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.mark_chat_as_read(chat_id);
            }
            self.dismiss_os_notification(chat_id, cx);
            return;
        }
        cx.activate(true);
        window.activate_window();
        self.select_listed_chat(chat_id, window, cx);
        if action == NotificationAction::Reply {
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
        }
    }

    /// Withdraw the chat's shown notification: GPUI dismisses by tag on
    /// macOS and Windows. Linux `notify-send --wait` processes own their
    /// notification and cannot be recalled; the daemon expires them.
    pub(super) fn dismiss_os_notification(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if quill::notify::current_backend() != quill::notify::NotifyBackend::Native {
            return;
        }
        let account = self
            .session()
            .map(|s| s.account.0.as_str())
            .unwrap_or("primary");
        cx.dismiss_system_notification(&quill::notify::notification_tag(account, chat_id));
    }

    /// Parity slice: resolve a notification sound and play it. `Default`
    /// plays the synthesized tone; a saved sound plays its MP3 once
    /// downloaded (a pending download plays on completion via
    /// `pending_sound_plays`). Silent when the chat's sound is disabled —
    /// the reducer never queues a sound for those.
    pub(super) fn play_notification_sound(&mut self, kind: NotificationSoundKind) {
        let sound = match self.live.as_mut() {
            Some(live) => notification_sound(live.driver.resolve_notification_sound(kind)),
            // No live driver (screenshot demo): the tone is the honest
            // stand-in — no TDLib file is reachable.
            None => Some(NotificationSound::DefaultTone),
        };
        if let Some(sound) = sound {
            self.notification_sounds.play(sound);
        }
    }

    /// Phase 8.1: show one queued notification via the platform backend on a
    /// worker thread. A click (Linux `notify-send --wait --action`) records
    /// the chat id; the next render focuses it. Concurrent workers are
    /// capped; excess bursts are dropped rather than stacking threads.
    pub(super) fn spawn_os_notification(
        &mut self,
        queued: QueuedNotification,
        cx: &mut Context<Self>,
    ) {
        // A locked app shows no sender or text (tdesktop hides the message
        // preview while the passcode lock is up).
        let notification = if self.passcode_ui.locked {
            queued.for_locked_display()
        } else {
            queued.for_display()
        };
        if quill::notify::current_backend() == quill::notify::NotifyBackend::Native {
            // macOS (UNUserNotificationCenter) and Windows (WinRT toast): the
            // click returns through `on_system_notification_response`.
            let account = self
                .session()
                .map(|s| s.account.0.as_str())
                .unwrap_or("primary");
            cx.show_system_notification(SystemNotification {
                tag: quill::notify::notification_tag(account, notification.chat_id).into(),
                title: notification.title.into(),
                body: notification.body.into(),
                // Locked: no buttons, a reply must not bypass the passcode.
                actions: quill::notify::action_buttons(self.passcode_ui.locked)
                    .into_iter()
                    .map(|(id, label)| SystemNotificationAction {
                        id: id.into(),
                        label: label.into(),
                    })
                    .collect(),
            });
            return;
        }
        let Some(command) = quill::notify::build_notification_command(&notification) else {
            return;
        };
        if self.notify_inflight.fetch_add(1, Ordering::SeqCst) >= MAX_OS_NOTIFICATION_THREADS {
            self.notify_inflight.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let clicks = self.notify_clicks.clone();
        let inflight = self.notify_inflight.clone();
        let spawn = std::thread::Builder::new()
            .name("quill-notify".to_string())
            .spawn(move || {
                let outcome = quill::notify::run_notification_command(&command);
                inflight.fetch_sub(1, Ordering::SeqCst);
                if let Some(action) = outcome.action
                    && let Ok(mut guard) = clicks.lock()
                {
                    guard.push((notification.chat_id, action));
                }
            });
        if spawn.is_err() {
            self.notify_inflight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub(super) fn finish_successful_sends(&mut self, cx: &mut Context<Self>) {
        let clears = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.draft_clears))
            .unwrap_or_default();
        if clears.is_empty() {
            return;
        }
        let idle = self.pending_edit.is_none()
            && self.pending_reply.is_none()
            && self.composer.read(cx).value().trim().is_empty();
        for chat_id in clears {
            if self.clear_draft_on_success != Some(chat_id) {
                continue;
            }
            self.clear_draft_on_success = None;
            if !idle {
                continue;
            }
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.clear_draft_after_send(chat_id, true);
            }
        }
    }
}

/// Slice auth-logout-warning D1 fix-up: whether this poll restarts the
/// live connection. `saw_logging_out` is latched through the drain loop
/// (not just `prev_auth`) so a `LoggingOut → Closed` batch arriving in
/// one poll still triggers the restart. The quit path never passes
/// through `LoggingOut`, so it never restarts.
fn logout_restart_trigger(saw_logging_out: bool, new_auth: &AuthorizationState) -> bool {
    saw_logging_out && matches!(new_auth, AuthorizationState::Closed)
}

/// Whether the poll should declare the receive bridge lost: it stopped on
/// its own, its queue is fully drained, and it was not flagged already.
fn bridge_lost(stopped: bool, budget_hit: bool, already_lost: bool) -> bool {
    stopped && !budget_hit && !already_lost
}

#[cfg(test)]
mod tests {
    use super::{PolledAction, PolledRedraw, bridge_lost, logout_restart_trigger};
    use quill::state::RedrawNeed;
    use quill::telegram::envelope::AuthorizationState;
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn nothing_new_never_redraws() {
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        for step in 0..100 {
            let at = start + ms(step * 120);
            assert_eq!(
                redraw.decide(RedrawNeed::Nothing, true, at),
                PolledAction::Wait
            );
        }
    }

    #[test]
    fn urgent_updates_redraw_everything_at_once() {
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        assert_eq!(
            redraw.decide(RedrawNeed::Now, true, start + ms(1)),
            PolledAction::Full
        );
        assert_eq!(
            redraw.decide(RedrawNeed::Now, true, start + ms(2)),
            PolledAction::Full
        );
    }

    #[test]
    fn a_stream_of_chat_list_updates_redraws_the_list_a_few_times_a_second() {
        // Ten presence / typing updates a second for ten seconds, polled
        // every 10 ms in between: the chat list redraws at most every
        // 400 ms, the conversation never.
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        let (mut lists, mut fulls) = (0, 0);
        for tick in 1..=1000_u64 {
            let need = if tick % 10 == 0 {
                RedrawNeed::ChatList
            } else {
                RedrawNeed::Nothing
            };
            match redraw.decide(need, true, start + ms(tick * 10)) {
                PolledAction::ChatList => lists += 1,
                PolledAction::Full => fulls += 1,
                PolledAction::Wait => {}
            }
        }
        assert_eq!(fulls, 0);
        assert!((20..=25).contains(&lists), "{lists} chat-list redraws");
    }

    #[test]
    fn a_held_back_redraw_lands_on_a_later_empty_poll() {
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        assert_eq!(
            redraw.decide(RedrawNeed::Later, true, start + ms(100)),
            PolledAction::Wait
        );
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, true, start + ms(900)),
            PolledAction::Wait
        );
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, true, start + ms(1000)),
            PolledAction::Full
        );
        // Drawn: nothing left over.
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, true, start + ms(5000)),
            PolledAction::Wait
        );
    }

    #[test]
    fn a_pending_chat_list_redraw_becomes_full_when_something_else_comes() {
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        assert_eq!(
            redraw.decide(RedrawNeed::ChatList, true, start + ms(10)),
            PolledAction::Wait
        );
        assert_eq!(
            redraw.decide(RedrawNeed::Now, true, start + ms(20)),
            PolledAction::Full
        );
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, true, start + ms(500)),
            PolledAction::Wait
        );
    }

    #[test]
    fn behind_another_app_batches_stretch() {
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        assert_eq!(
            redraw.decide(RedrawNeed::ChatList, false, start + ms(1000)),
            PolledAction::Wait
        );
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, false, start + ms(2000)),
            PolledAction::ChatList
        );
        // Urgent updates still redraw every 500 ms.
        assert_eq!(
            redraw.decide(RedrawNeed::Now, false, start + ms(2100)),
            PolledAction::Full
        );
        assert_eq!(
            redraw.decide(RedrawNeed::Now, false, start + ms(2200)),
            PolledAction::Wait
        );
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, false, start + ms(2600)),
            PolledAction::Full
        );
    }

    #[test]
    fn logout_restart_trigger_covers_batched_transition() {
        // D1: `LoggingOut` and `Closed` batched in one poll — the latch
        // saw LoggingOut even though prev_auth was Ready.
        assert!(logout_restart_trigger(true, &AuthorizationState::Closed));
        // Quit path: Closed without ever seeing LoggingOut — no restart.
        assert!(!logout_restart_trigger(false, &AuthorizationState::Closed));
        // LogOut sent but Closed not reached yet — keep polling.
        assert!(!logout_restart_trigger(
            true,
            &AuthorizationState::LoggingOut
        ));
        assert!(!logout_restart_trigger(false, &AuthorizationState::Ready));
    }

    #[test]
    fn a_pending_full_redraw_does_not_hold_the_chat_list_back() {
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        assert_eq!(
            redraw.decide(RedrawNeed::Later, true, start + ms(100)),
            PolledAction::Wait
        );
        assert_eq!(
            redraw.decide(RedrawNeed::ChatList, true, start + ms(200)),
            PolledAction::Wait
        );
        // The chat list goes out on its own interval...
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, true, start + ms(400)),
            PolledAction::ChatList
        );
        // ...and the full redraw still lands on its own.
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, true, start + ms(1000)),
            PolledAction::Full
        );
    }

    #[test]
    fn delivering_draws_at_once_even_behind_another_app() {
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        assert_eq!(
            redraw.decide(RedrawNeed::Now, false, start + ms(600)),
            PolledAction::Full
        );
        // A notification queued 10 ms later does not wait out the
        // inactive interval, and takes everything pending with it.
        assert_eq!(
            redraw.decide(RedrawNeed::ChatList, false, start + ms(605)),
            PolledAction::Wait
        );
        assert_eq!(redraw.drawn(start + ms(610)), PolledAction::Full);
        assert_eq!(
            redraw.decide(RedrawNeed::Nothing, false, start + ms(5000)),
            PolledAction::Wait
        );
    }

    /// Feeds `PolledRedraw` a pseudo-random stream of needs, polled the
    /// way the poll loop does (10–120 ms apart, with the window going to
    /// the background and back), and checks that every need is covered
    /// by a redraw no later than its interval allows: held back, never
    /// dropped.
    #[test]
    fn every_need_is_drawn_within_its_interval() {
        let start = Instant::now();
        let mut redraw = PolledRedraw::new(start);
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let mut at = start;
        // The oldest need not covered yet: (when, need, active then).
        let mut owed_list: Option<Instant> = None;
        let mut owed_full: Option<(Instant, RedrawNeed)> = None;
        let mut active = true;
        for _ in 0..20_000 {
            at += ms(10 + next(111));
            if next(200) == 0 {
                active = !active;
            }
            let need = match next(10) {
                0..=4 => RedrawNeed::Nothing,
                5..=7 => RedrawNeed::ChatList,
                8 => RedrawNeed::Later,
                _ => RedrawNeed::Now,
            };
            match need {
                RedrawNeed::ChatList => {
                    owed_list.get_or_insert(at);
                }
                RedrawNeed::Later | RedrawNeed::Now => {
                    let owed = owed_full.get_or_insert((at, need));
                    owed.1 = owed.1.max(need);
                }
                RedrawNeed::Nothing => {}
            }
            match redraw.decide(need, active, at) {
                PolledAction::Full => {
                    owed_list = None;
                    owed_full = None;
                }
                PolledAction::ChatList => owed_list = None,
                PolledAction::Wait => {}
            }
            // The longest an owed redraw may wait, plus one poll gap.
            let slack = ms(121);
            if let Some(since) = owed_list {
                let (on, off) = super::CHAT_LIST_REDRAW;
                let limit = if active { on } else { off };
                assert!(
                    at - since <= limit + slack,
                    "chat list held {:?}",
                    at - since
                );
            }
            if let Some((since, need)) = owed_full {
                let limit = match (need, active) {
                    (RedrawNeed::Now, true) => Duration::ZERO,
                    (RedrawNeed::Now, false) => super::INACTIVE_REDRAW,
                    (_, true) => super::LATER_REDRAW.0,
                    (_, false) => super::LATER_REDRAW.1,
                };
                assert!(
                    at - since <= limit + slack,
                    "{need:?} held {:?} (active: {active})",
                    at - since
                );
            }
        }
    }

    #[test]
    fn bridge_loss_waits_for_the_queue_and_fires_once() {
        assert!(bridge_lost(true, false, false));
        assert!(!bridge_lost(false, false, false));
        // Updates are still queued behind the ingest budget.
        assert!(!bridge_lost(true, true, false));
        assert!(!bridge_lost(true, false, true));
    }
}
