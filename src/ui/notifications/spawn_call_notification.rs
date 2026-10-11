//! Methods moved out of `notifications.rs` to keep files under 1000 lines.

use super::*;
use quill::ids::AccountKey;

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
    /// also focuses the composer (a Reply whose text was typed into the
    /// notification was already sent by `drain_background_clicks`).
    pub(in crate::ui) fn run_notification_action(
        &mut self,
        chat_id: ChatId,
        action: NotificationAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action == NotificationAction::MarkRead {
            self.mark_read_from_notification(chat_id, cx);
            return;
        }
        // A chat with a window of its own takes the click (Telegram
        // Desktop `windowForShowingHistory`); the main window stays behind.
        if self.routed_to_chat_window(chat_id, cx) {
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
    /// macOS and Windows; on Linux `CloseNotification` over D-Bus from a
    /// worker thread. A `notify-send --wait` fallback process owns its
    /// notification and cannot be recalled; the daemon expires it.
    pub(in crate::ui) fn dismiss_os_notification(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_notification_as(None, chat_id, cx);
    }

    /// [`Self::dismiss_os_notification`] for the account on screen
    /// (`account` `None`) or a background account.
    pub(in crate::ui) fn dismiss_notification_as(
        &mut self,
        account: Option<&AccountKey>,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        if account.is_none() {
            self.notify.reply_targets.remove(&chat_id);
        }
        if quill::notify::current_backend() == quill::notify::NotifyBackend::Native {
            let tag = match account {
                Some(key) => quill::notify::notification_tag(&key.0, chat_id),
                None => self.notification_tag(chat_id),
            };
            cx.dismiss_system_notification(&tag);
            return;
        }
        self.close_linux_notification(account.cloned(), chat_id);
    }

    /// Linux: `CloseNotification` for the chat's toast on a worker thread,
    /// when the D-Bus notifier is up. Nothing elsewhere.
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables, clippy::unused_self))]
    fn close_linux_notification(&self, account: Option<AccountKey>, chat_id: ChatId) {
        #[cfg(target_os = "linux")]
        if let Some(notifier) =
            super::linux_dbus::notifier(&self.notify.notify_clicks, &self.background.clicks)
        {
            let _ = std::thread::Builder::new()
                .name("quill-notify-close".to_string())
                .spawn(move || {
                    if let Err(error) = notifier.close(&(account, chat_id)) {
                        eprintln!("failed to close the desktop notification: {error}");
                    }
                });
        }
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

    /// Phase 8.1: show one queued notification via the platform backend.
    /// macOS and Windows go through GPUI; Linux through D-Bus (or the
    /// `notify-send` fallback) on a worker thread. A click records the chat
    /// id; the next render focuses it. Concurrent workers are capped; excess
    /// bursts are dropped rather than stacking threads.
    pub(in crate::ui) fn spawn_os_notification(
        &mut self,
        queued: QueuedNotification,
        cx: &mut Context<Self>,
    ) {
        // A locked app shows no sender or text (tdesktop hides the message
        // preview while the passcode lock is up).
        let locked = self.account.passcode.locked;
        let notification = if locked {
            queued.for_locked_display()
        } else {
            queued.for_display()
        };
        let icon = self.notification_avatar(notification.chat_id, locked);
        self.show_os_notification(notification, icon, locked, cx);
    }

    /// The chat's circular avatar PNG for its notification: only an
    /// already-downloaded small photo, and never while locked (the locked
    /// toast hides the sender).
    fn notification_avatar(&self, chat_id: ChatId, locked: bool) -> Option<PathBuf> {
        if locked {
            return None;
        }
        self.session()
            .and_then(|s| s.chat_photo_path(chat_id))
            .map(PathBuf::from)
            .and_then(|path| super::avatar_icon::circular_icon_for(&path))
    }

    /// Show one notification with its avatar through the platform backend
    /// (see [`Self::spawn_os_notification`]).
    pub(in crate::ui) fn show_os_notification(
        &mut self,
        notification: quill::notify::OsNotification,
        icon: Option<PathBuf>,
        locked: bool,
        cx: &mut Context<Self>,
    ) {
        self.show_notification_as(None, notification, icon, locked, cx);
    }

    /// [`Self::show_os_notification`] for the account on screen (`account`
    /// `None`) or a background account. A background account's
    /// notification has no reply field (a reply needs that account on
    /// screen: Reply switches to it and focuses the composer), and its
    /// clicks go to `background.clicks`.
    pub(in crate::ui) fn show_notification_as(
        &mut self,
        account: Option<&AccountKey>,
        notification: quill::notify::OsNotification,
        icon: Option<PathBuf>,
        locked: bool,
        cx: &mut Context<Self>,
    ) {
        let chat_id = notification.chat_id;
        let reply_field = account.is_none() && !locked;
        // An inline reply answers the newest notified message.
        if account.is_none() {
            match notification.message_id {
                Some(message_id) => self.notify.reply_targets.insert(chat_id, message_id),
                None => self.notify.reply_targets.remove(&chat_id),
            };
        }
        if quill::notify::current_backend() == quill::notify::NotifyBackend::Native {
            // macOS (UNUserNotificationCenter) and Windows (WinRT toast): the
            // click returns through `on_system_notification_response`. The
            // picture and the reply field ride beside the GPUI notification;
            // a locked app offers no buttons, so no field either.
            let tag = match account {
                Some(key) => quill::notify::notification_tag(&key.0, chat_id),
                None => self.notification_tag(chat_id),
            };
            super::native_extras::register(
                &tag,
                icon.as_deref(),
                reply_field.then_some(NotificationAction::Reply.id()),
            );
            cx.show_system_notification(SystemNotification {
                tag: tag.into(),
                title: notification.title.into(),
                body: notification.body.into(),
                // Locked: no buttons, a reply must not bypass the passcode.
                actions: quill::notify::action_buttons(locked)
                    .into_iter()
                    .map(|(id, label)| SystemNotificationAction {
                        id: id.into(),
                        label: label.into(),
                    })
                    .collect(),
            });
            return;
        }
        let Some(command) =
            quill::notify::build_notification_command_with_icon(&notification, icon.as_deref())
        else {
            return;
        };
        if self.notify.notify_inflight.fetch_add(1, Ordering::SeqCst) >= MAX_OS_NOTIFICATION_THREADS
        {
            self.notify.notify_inflight.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let clicks = self.notify.notify_clicks.clone();
        let account_clicks = self.background.clicks.clone();
        let account = account.cloned();
        let inflight = self.notify.notify_inflight.clone();
        let spawn = std::thread::Builder::new()
            .name("quill-notify".to_string())
            .spawn(move || {
                // D-Bus first: `Notify` returns at once and the listener
                // thread reports the click. Without a session bus,
                // `notify-send --wait` holds the thread until the click.
                #[cfg(target_os = "linux")]
                if let Some(notifier) = super::linux_dbus::notifier(&clicks, &account_clicks) {
                    if let Err(error) = notifier.show(
                        (account, chat_id),
                        &notification.title,
                        &notification.body,
                        icon.as_deref(),
                        locked,
                        reply_field,
                    ) {
                        eprintln!("failed to show the desktop notification: {error}");
                    }
                    inflight.fetch_sub(1, Ordering::SeqCst);
                    return;
                }
                let outcome = quill::notify::run_notification_command(&command);
                inflight.fetch_sub(1, Ordering::SeqCst);
                let Some(action) = outcome.action else {
                    return;
                };
                match account {
                    Some(account) => {
                        if let Ok(mut guard) = account_clicks.lock() {
                            guard.push((account, chat_id, action));
                        }
                    }
                    None => {
                        if let Ok(mut guard) = clicks.lock() {
                            guard.push((chat_id, action));
                        }
                    }
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
        for (chat_id, topic) in clears {
            if self.composer_ui.clear_draft_on_success != Some(chat_id) {
                continue;
            }
            self.composer_ui.clear_draft_on_success = None;
            if !idle {
                continue;
            }
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.clear_draft_after_send(chat_id, topic, true);
            }
        }
    }
}
