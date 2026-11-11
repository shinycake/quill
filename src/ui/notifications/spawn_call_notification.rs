//! Methods moved out of `notifications.rs` to keep files under 1000 lines.

use super::*;
use quill::ids::AccountKey;

impl QuillApp {
    /// The incoming-call notification of `account` (the one on screen, or
    /// one in the background whose call rings).
    pub(in crate::ui) fn spawn_call_notification_as(
        &mut self,
        account: &AccountKey,
        notification: quill::notify_call::CallNotification,
        cx: &mut Context<Self>,
    ) {
        if quill::notify::current_backend() == quill::notify::NotifyBackend::Native {
            let account = account.0.as_str();
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
        let Some(slot) = quill::thread_guard::InflightSlot::acquire(
            &self.delivery.notify_inflight,
            MAX_OS_NOTIFICATION_THREADS,
        ) else {
            return;
        };
        let clicks = self.calls.notify_clicks.clone();
        let call_id = notification.call_id;
        let account = account.clone();
        let spawn = quill::thread_guard::spawn_guarded("quill-call-notify", move || {
            let action = quill::notify_call::run_call_command(&command);
            drop(slot);
            if let Some(action) = action
                && let Ok(mut guard) = clicks.lock()
            {
                guard.push(quill::notify_call::CallPick {
                    account,
                    call_id,
                    action,
                });
                // The poll loop sleeps while idle; wake it for the click.
                quill::wake::UPDATES.poke();
            }
        });
        // A thread that never started drops `slot` with its closure.
        drop(spawn);
    }

    /// A notification was clicked or one of its buttons pressed. "Mark as
    /// read" works in the background (the window stays where it is); "Open"
    /// and "Reply" bring the app forward with the chat selected, and Reply
    /// also focuses the composer. A Reply whose text was typed into the
    /// notification sends it instead: `drain_background_clicks` did for the
    /// account on screen, and here does for a click that switched accounts
    /// first (`open_pending_account_chat`).
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
        if action == NotificationAction::Reply
            && let Some(account) = self.current_account()
            && self.send_typed_notification_reply(&account, chat_id, cx)
        {
            return;
        }
        // Behind the passcode the window comes forward and nothing opens
        // (Telegram Desktop `openNotificationMessage`).
        if self.account.passcode.is_locked() {
            cx.activate(true);
            window.activate_window();
            return;
        }
        // A chat with a window of its own takes the click (Telegram
        // Desktop `windowForShowingHistory`); the main window stays behind.
        if self.routed_to_chat_window(chat_id, cx) {
            // The text typed into the toast goes to that window's composer.
            if let Some(account) = self.current_account() {
                self.in_window_scope(Some(chat_id), cx, |app, cx| {
                    app.compose_unsent_reply(&account, chat_id, None, cx);
                });
            }
            return;
        }
        cx.activate(true);
        window.activate_window();
        // The viewer does not stay over the chat (`MainWidget::showHistory`).
        if self.viewer.state.is_open() {
            self.close_media_viewer(cx);
        }
        self.select_listed_chat(chat_id, window, cx);
        // A kit dialog keeps the keys until it closes.
        if let Some(account) = self.current_account() {
            self.compose_unsent_reply(&account, chat_id, Some(window), cx);
        }
        if self.open_chat_id() == Some(chat_id) && !self.kit_dialog_open() {
            self.focus_opened_chat(window, cx);
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
        let account = self.notification_account(account);
        let key = (account, chat_id);
        self.delivery.reply_targets.remove(&key);
        // One still waiting for its photo never shows.
        self.delivery.preview_waits.remove(&key);
        let (account, chat_id) = key;
        // Quill's own windows, whichever backend showed the rest.
        self.clear_custom_notifications(&account, chat_id, cx);
        if quill::notify::current_backend() == quill::notify::NotifyBackend::Native {
            cx.dismiss_system_notification(&quill::notify::notification_tag(&account.0, chat_id));
            return;
        }
        self.close_linux_notification(account, chat_id);
    }

    /// The account a notification is for: `account`, or the one on screen
    /// when `None` ("primary" before any session). Every shown
    /// notification keeps it, so a click or reply after an account switch
    /// still knows whose it was.
    pub(in crate::ui) fn notification_account(&self, account: Option<&AccountKey>) -> AccountKey {
        account
            .cloned()
            .or_else(|| self.session().map(|s| s.account.clone()))
            .unwrap_or_else(AccountKey::primary)
    }

    /// Linux: `CloseNotification` for the chat's toast on a worker thread,
    /// when a worker already connected the D-Bus notifier: this runs on the
    /// UI thread, which never connects (round 18 B #6). Nothing elsewhere.
    #[allow(clippy::unused_self)]
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    fn close_linux_notification(&self, account: AccountKey, chat_id: ChatId) {
        #[cfg(target_os = "linux")]
        if let Some(notifier) = super::linux_dbus::connected_notifier() {
            let _ = quill::thread_guard::spawn_guarded("quill-notify-close", move || {
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
    pub(in crate::ui) fn play_notification_sound(
        &mut self,
        kind: NotificationSoundKind,
        chat_id: ChatId,
    ) {
        let volume = self
            .session()
            .and_then(|session| session.notification_volume_override(chat_id));
        let sound = match self.live.as_mut() {
            Some(live) => notification_sound(live.driver.resolve_notification_sound(kind)),
            // No live driver (screenshot demo): the tone is the honest
            // stand-in — no TDLib file is reachable.
            None => Some(NotificationSound::DefaultTone),
        };
        if let Some(sound) = sound {
            self.delivery.notification_sounds.play_at(sound, volume);
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
        // preview while the passcode lock is up), and neither does the
        // "Show sender name" switch when off: no picture, no buttons.
        let locked = self.account.passcode.is_locked()
            || self
                .session()
                .is_some_and(|s| !s.settings.notify_view.shows_name());
        let notification = if locked {
            queued.for_locked_display()
        } else {
            queued.for_display()
        };
        let photo = self.notification_photo(notification.chat_id, locked);
        let preview = queued.photo.filter(|_| !locked);
        self.show_notification_with_preview(None, notification, photo, preview, locked, cx);
    }

    /// The chat's already-downloaded small photo for its notification's
    /// avatar, never while locked (the locked toast hides the sender).
    fn notification_photo(&self, chat_id: ChatId, locked: bool) -> Option<PathBuf> {
        if locked {
            return None;
        }
        self.session()
            .and_then(|s| s.chat_photo_path(chat_id))
            .map(PathBuf::from)
    }

    /// Show one notification with its avatar through the platform backend
    /// (see [`Self::spawn_os_notification`]).
    pub(in crate::ui) fn show_os_notification(
        &mut self,
        notification: quill::notify::OsNotification,
        icon: Option<PathBuf>,
        preview: Option<PathBuf>,
        locked: bool,
        cx: &mut Context<Self>,
    ) {
        self.show_notification_as(None, notification, icon, preview, locked, cx);
    }

    /// [`Self::show_os_notification`] for the account on screen (`account`
    /// `None`) or a named account. The notification keeps the account it
    /// was shown for, and its clicks run on that account (`ChatClick`). One
    /// for an account not on screen has no reply field (a reply needs that
    /// account on screen: Reply switches to it and focuses the composer).
    /// `preview` is the message's photo ("Show photo previews").
    pub(in crate::ui) fn show_notification_as(
        &mut self,
        account: Option<&AccountKey>,
        notification: quill::notify::OsNotification,
        icon: Option<PathBuf>,
        preview: Option<PathBuf>,
        locked: bool,
        cx: &mut Context<Self>,
    ) {
        let notification = notification.shown();
        let chat_id = notification.chat_id;
        let account = self.notification_account(account);
        let on_screen = self.session().is_some_and(|s| s.account == account);
        let offers_actions = notification.offers_actions(locked);
        let reply_field = on_screen && offers_actions;
        // An inline reply answers the newest notified message.
        if on_screen {
            let key = (account.clone(), chat_id);
            match notification.message_id {
                Some(message_id) => self.delivery.reply_targets.insert(key, message_id),
                None => self.delivery.reply_targets.remove(&key),
            };
        }
        if self.custom_notifications_active() {
            // Quill's own windows (`notify_windows`); Windows' "Respect
            // system Focus mode" skips them while Focus is on.
            let skip = cfg!(windows)
                && quill::notify_prefs::current().skip_in_focus
                && quill::notify_focus::dnd_active();
            if !skip {
                self.show_custom_notification(
                    &account,
                    notification,
                    icon,
                    preview,
                    locked,
                    reply_field,
                    cx,
                );
            }
            return;
        }
        if quill::notify::current_backend() == quill::notify::NotifyBackend::Native {
            // macOS (UNUserNotificationCenter) and Windows (WinRT toast): the
            // click returns through `on_system_notification_response`. The
            // picture and the reply field ride beside the GPUI notification;
            // a locked app offers no buttons, so no field either.
            let tag = quill::notify::notification_tag(&account.0, chat_id);
            super::native_extras::register(
                &tag,
                icon.as_deref(),
                preview.as_deref(),
                reply_field.then_some(NotificationAction::Reply.id()),
            );
            let actions = quill::notify::action_buttons(&notification, locked);
            cx.show_system_notification(SystemNotification {
                tag: tag.into(),
                title: notification.title.into(),
                body: notification.body.into(),
                actions: actions
                    .into_iter()
                    .map(|(id, label)| SystemNotificationAction {
                        id: id.into(),
                        label: label.into(),
                    })
                    .collect(),
            });
            return;
        }
        // `notify-send` has one picture: the photo, when there is one.
        let Some(command) = quill::notify::build_notification_command_with_icon(
            &notification,
            preview.as_deref().or(icon.as_deref()),
            locked,
        ) else {
            return;
        };
        let Some(slot) = quill::thread_guard::InflightSlot::acquire(
            &self.delivery.notify_inflight,
            MAX_OS_NOTIFICATION_THREADS,
        ) else {
            return;
        };
        // Every click goes to `background.clicks` with the account, and
        // `drain_account_clicks` runs it on that account.
        let clicks = self.background.clicks.clone();
        let spawn = quill::thread_guard::spawn_guarded("quill-notify", move || {
            // D-Bus first: `Notify` returns at once and the listener
            // thread reports the click. Without a session bus,
            // `notify-send --wait` holds the thread until the click.
            #[cfg(target_os = "linux")]
            if let Some(notifier) = super::linux_dbus::notifier_on_worker(&clicks) {
                if let Err(error) = notifier.show(
                    (account, chat_id),
                    &notification.title,
                    &notification.body,
                    icon.as_deref(),
                    preview.as_deref(),
                    !offers_actions,
                    reply_field,
                ) {
                    eprintln!("failed to show the desktop notification: {error}");
                }
                return;
            }
            let outcome = quill::notify::run_notification_command(&command);
            drop(slot);
            let Some(action) = outcome.action else {
                return;
            };
            if let Ok(mut guard) = clicks.lock() {
                guard.push(quill::notify_accounts::ChatClick {
                    account,
                    chat_id,
                    action,
                });
            }
            quill::wake::UPDATES.poke();
        });
        // A thread that never started drops `slot` with its closure.
        drop(spawn);
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
        let open_chat = self.open_chat_id();
        let composer_idle = !self.chat_ui.is_editing()
            && self.chat_ui.reply().is_none()
            && self.composer.read(cx).value().trim().is_empty();
        for (chat_id, topic) in clears {
            let Some(flag) = self.composer_ui.clear_draft_on_success.clone() else {
                continue;
            };
            let stored = self
                .session()
                .and_then(|session| session.draft_for(chat_id, topic))
                .map(|draft| draft.text.clone());
            if !flag.matches(chat_id, stored.as_deref()) {
                continue;
            }
            self.composer_ui.clear_draft_on_success = None;
            let idle = if open_chat == Some(chat_id) {
                composer_idle
            } else {
                true
            };
            if !idle {
                continue;
            }
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.clear_draft_after_send(chat_id, topic, true);
            }
        }
    }
}
