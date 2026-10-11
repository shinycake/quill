//! Methods moved out of `notifications.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(super) fn spawn_call_notification(
        &mut self,
        notification: quill::notify_call::CallNotification,
        cx: &mut Context<Self>,
    ) {
        if quill::notify::current_backend() == quill::notify::NotifyBackend::Native {
            let account = self
                .session()
                .map(|s| s.account.0.as_str())
                .unwrap_or("primary");
            cx.show_system_notification(SystemNotification {
                tag: quill::notify_call::call_notification_tag(account, notification.call_id)
                    .into(),
                title: notification.title.clone().into(),
                body: notification.body.clone().into(),
                actions: quill::notify_call::call_buttons(&notification)
                    .into_iter()
                    .map(|(id, label)| SystemNotificationAction {
                        id: id.into(),
                        label: label.into(),
                    })
                    .collect(),
            });
            return;
        }
        let Some(command) = quill::notify_call::build_call_command(&notification) else {
            return;
        };
        if self.notify.notify_inflight.fetch_add(1, Ordering::SeqCst) >= MAX_OS_NOTIFICATION_THREADS
        {
            self.notify.notify_inflight.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let clicks = self.calls.notify_clicks.clone();
        let inflight = self.notify.notify_inflight.clone();
        let call_id = notification.call_id;
        let spawn = std::thread::Builder::new()
            .name("quill-call-notify".to_string())
            .spawn(move || {
                let action = quill::notify_call::run_call_command(&command);
                inflight.fetch_sub(1, Ordering::SeqCst);
                if let Some(action) = action
                    && let Ok(mut guard) = clicks.lock()
                {
                    guard.push((call_id, action));
                }
            });
        if spawn.is_err() {
            self.notify.notify_inflight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// A notification was clicked or one of its buttons pressed. "Mark as
    /// read" works in the background (the window stays where it is); "Open"
    /// and "Reply" bring the app forward with the chat selected, and Reply
    /// also focuses the composer (GPUI notifications have no inline text
    /// field, see `quill::notify::NotificationAction`).
    pub(in crate::ui) fn run_notification_action(
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
    pub(in crate::ui) fn dismiss_os_notification(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
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
    pub(in crate::ui) fn play_notification_sound(&mut self, kind: NotificationSoundKind) {
        let sound = match self.live.as_mut() {
            Some(live) => notification_sound(live.driver.resolve_notification_sound(kind)),
            // No live driver (screenshot demo): the tone is the honest
            // stand-in — no TDLib file is reachable.
            None => Some(NotificationSound::DefaultTone),
        };
        if let Some(sound) = sound {
            self.notify.notification_sounds.play(sound);
        }
    }

    /// Phase 8.1: show one queued notification via the platform backend on a
    /// worker thread. A click (Linux `notify-send --wait --action`) records
    /// the chat id; the next render focuses it. Concurrent workers are
    /// capped; excess bursts are dropped rather than stacking threads.
    pub(in crate::ui) fn spawn_os_notification(
        &mut self,
        queued: QueuedNotification,
        cx: &mut Context<Self>,
    ) {
        // A locked app shows no sender or text (tdesktop hides the message
        // preview while the passcode lock is up).
        let notification = if self.account.passcode.locked {
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
                actions: quill::notify::action_buttons(self.account.passcode.locked)
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
        if self.notify.notify_inflight.fetch_add(1, Ordering::SeqCst) >= MAX_OS_NOTIFICATION_THREADS
        {
            self.notify.notify_inflight.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let clicks = self.notify.notify_clicks.clone();
        let inflight = self.notify.notify_inflight.clone();
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
            self.notify.notify_inflight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub(in crate::ui) fn finish_successful_sends(&mut self, cx: &mut Context<Self>) {
        let clears = self
            .live
            .as_mut()
            .map(|live| std::mem::take(&mut live.driver.session.messages.draft_clears))
            .unwrap_or_default();
        if clears.is_empty() {
            return;
        }
        let idle = self.composer_ui.pending_edit.is_none()
            && self.composer_ui.pending_reply.is_none()
            && self.composer.read(cx).value().trim().is_empty();
        for chat_id in clears {
            if self.composer_ui.clear_draft_on_success != Some(chat_id) {
                continue;
            }
            self.composer_ui.clear_draft_on_success = None;
            if !idle {
                continue;
            }
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.clear_draft_after_send(chat_id, true);
            }
        }
    }
}
