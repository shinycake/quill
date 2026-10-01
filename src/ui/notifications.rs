//! poll loop, OS notification threads, push_status_note.

use super::app::QuillApp;
use super::connect_ui::{bootstrap_connect, live_status_for};
use super::notification_settings::{
    MAX_OS_NOTIFICATION_SOUND_THREADS, MAX_OS_NOTIFICATION_THREADS,
};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::connect::SoundResolution;
use quill::ids::ChatId;
use quill::notify::{NotificationSoundKind, QueuedNotification};
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

impl QuillApp {
    /// kit Phase 2 (redo): the hand-rolled toast pill is gone — new
    /// `status_note`s go to the kit notification layer (auto-dismiss,
    /// stacking, theme from `cx.theme()`). Call sites keep writing
    /// `status_note`; render consumes it here.
    pub(super) fn push_status_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.status_note.is_empty() {
            return;
        }
        let note = std::mem::take(&mut self.status_note);
        window.push_notification(Notification::info(note), cx);
    }
}

impl QuillApp {
    pub(super) fn spawn_poll_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        this.poll_live(cx);
                        this.live.is_some()
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
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

    pub(super) fn poll_live(&mut self, cx: &mut Context<Self>) {
        self.apply_pending_keybindings(cx);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let prev_auth = live.driver.session.auth.clone();
        let mut progressed = live
            .driver
            .session
            .expire_pending_bot_messages(quill::state::unix_ms_now());
        let mut send_failed = false;
        // Slice auth-logout-warning D1 fix-up: latch `LoggingOut` inside
        // the drain loop. TDLib can queue both `LoggingOut` and `Closed`
        // before one poll runs; a `prev_auth`-only check then sees
        // Ready → Closed and misses the restart entirely.
        let mut saw_logging_out = matches!(prev_auth, AuthorizationState::LoggingOut);
        while let Some(owned) = live.bridge.next_timeout(Duration::from_millis(0)) {
            if live.driver.ingest(owned).is_err() {
                send_failed = true;
            }
            saw_logging_out = saw_logging_out
                || matches!(live.driver.session.auth, AuthorizationState::LoggingOut);
            progressed = true;
        }
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
        let chat_count = live.driver.session.ordered_chats().len();
        let chats_exhausted = live.driver.session.chats_exhausted;
        if send_failed {
            self.status_note = "failed to send TDLib request".into();
        } else if progressed {
            if let Some(err) = err {
                self.status_note = err.user_message().into();
            } else if new_auth != prev_auth {
                self.status_note = live_status_for(&new_auth);
            } else if matches!(new_auth, AuthorizationState::Ready) {
                self.status_note = if chats_exhausted {
                    format!("signed in — {chat_count} chats")
                } else {
                    format!("signed in — loading chats ({chat_count})")
                };
            }
        }
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
            self.status_note = "message link copied".into();
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
            .map(|live| live.driver.session.member_list_stale.drain(..).collect())
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
        self.finish_successful_sends(cx);
        // Bots slice: re-run the inline progress check after every batch
        // of session updates — arm the debounced dispatch for the current
        // trigger, if any. Idempotent: no-ops when the slot is already
        // fresh or a timer is armed.
        if progressed {
            self.progress_inline_mode(cx);
        }
        if progressed || send_failed {
            cx.notify();
        }
        self.resume_pending_gif(cx);
        self.resume_pending_video(cx);
        self.resume_pending_audio(cx);
        // Parity slice 5: a viewer video whose clip just finished downloading.
        self.resume_pending_viewer_video(cx);
        if logged_out {
            self.restart_live_connection(cx);
        }
    }

    /// Slice auth-logout-warning: drop the logged-out client and start a
    /// fresh connection — the auth flow resumes at
    /// `WaitTdlibParameters` → phone entry, i.e. the login screen. The
    /// old client is already Closed, so `Drop` joins its receive thread
    /// immediately instead of waiting out the 5s close timeout. If the
    /// restart fails the status line says so and the poll loop (which
    /// breaks on `live.is_none()`) stops.
    fn restart_live_connection(&mut self, cx: &mut Context<Self>) {
        let old = self.live.take();
        drop(old);
        // D2 fix-up: the warning promises "Downloaded media will be
        // erased from this device" — TDLib destroyed its own data, but
        // Quill's decrypted media scratch is only swept at startup.
        quill::local_path::sweep_media_caches();
        let (status, live, note, auth) = bootstrap_connect(self.credentials.clone());
        self.live = live;
        self.reload_account_keybindings(cx);
        self.connect_status = status;
        self.status_note = note;
        self.auth_demo = auth;
        cx.notify();
    }

    /// Phase 8.1: drain notification click callbacks (focus the chat) and
    /// dispatch newly queued notifications on worker threads. Runs from
    /// `render`, which is the only UI path with a `&mut Window`.
    pub(super) fn flush_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clicks: Vec<ChatId> = self
            .notify_clicks
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default();
        for chat_id in clicks {
            self.select_listed_chat(chat_id, window, cx);
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
            self.spawn_os_notification(queued);
        }
        // Parity slice: custom sounds whose downloads just completed.
        let plays: Vec<PathBuf> = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.pending_sound_plays))
            .unwrap_or_default();
        for path in plays {
            if let Some(command) = quill::notify::file_sound_command(&path.to_string_lossy()) {
                self.spawn_sound_command(command);
            }
        }
    }

    /// Parity slice: resolve a notification sound and play it. `Default`
    /// plays the synthesized tone; a saved sound plays its MP3 once
    /// downloaded (a pending download plays on completion via
    /// `pending_sound_plays`). Silent when the chat's sound is disabled —
    /// the reducer never queues a sound for those.
    pub(super) fn play_notification_sound(&mut self, kind: NotificationSoundKind) {
        let command = match self.live.as_mut() {
            Some(live) => match live.driver.resolve_notification_sound(kind) {
                SoundResolution::DefaultTone => quill::notify::default_tone_command(),
                SoundResolution::FilePath(path) => {
                    quill::notify::file_sound_command(&path.to_string_lossy())
                }
                SoundResolution::Pending => None,
            },
            // No live driver (screenshot demo): the tone is the honest
            // stand-in — no TDLib file is reachable.
            None => quill::notify::default_tone_command(),
        };
        if let Some(command) = command {
            self.spawn_sound_command(command);
        }
    }

    /// Parity slice: play one notification sound on a worker thread. A
    /// player failure (missing ffplay, vanished file) is silent by design —
    /// a notification must never surface an error dialog.
    pub(super) fn spawn_sound_command(&mut self, command: quill::notify::SoundCommand) {
        if self.notify_sound_inflight.fetch_add(1, Ordering::SeqCst)
            >= MAX_OS_NOTIFICATION_SOUND_THREADS
        {
            self.notify_sound_inflight.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let inflight = self.notify_sound_inflight.clone();
        let spawn = std::thread::Builder::new()
            .name("quill-sound".to_string())
            .spawn(move || {
                quill::notify::play_sound_command(&command);
                inflight.fetch_sub(1, Ordering::SeqCst);
            });
        if spawn.is_err() {
            self.notify_sound_inflight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Phase 8.1: show one queued notification via the platform backend on a
    /// worker thread. A click (Linux `notify-send --wait --action`) records
    /// the chat id; the next render focuses it. Concurrent workers are
    /// capped; excess bursts are dropped rather than stacking threads.
    pub(super) fn spawn_os_notification(&mut self, queued: QueuedNotification) {
        let notification = queued.for_display();
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
                if outcome.clicked
                    && let Ok(mut guard) = clicks.lock()
                {
                    guard.push(notification.chat_id);
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

#[cfg(test)]
mod tests {
    use super::logout_restart_trigger;
    use quill::telegram::envelope::AuthorizationState;

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
}
