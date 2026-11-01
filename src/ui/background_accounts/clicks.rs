//! Background accounts' notifications reaching the screen, and what a click
//! on one does: "Mark as read" reads the chat on that account in place;
//! Open and Reply switch to the account and then open the chat (tdesktop
//! `Window::Notifications::Manager::openNotificationMessage` activates the
//! notification's account first).

use super::*;
use quill::locks::LockExt;
use quill::notify_accounts::{ClickPlan, plan_click};
use quill::notify_focus::{AlertInput, dnd_active, plan_alert};
use quill::telegram::envelope::AuthorizationState;

/// How long a chat waits to be opened after a notification switched
/// accounts (a cold start restores the session first).
const PENDING_OPEN_FOR: Duration = Duration::from_secs(30);

impl QuillApp {
    /// Show what one background poll queued for `account`.
    pub(super) fn deliver_background(
        &mut self,
        account: &AccountKey,
        drain: NotifyDrain,
        cx: &mut Context<Self>,
    ) {
        for chat_id in drain.clears {
            self.dismiss_notification_as(Some(account), chat_id, cx);
        }
        self.background.attention |= drain.attention;
        let dnd = dnd_active();
        for queued in drain.notifications {
            let plan = plan_alert(AlertInput {
                flash_enabled: false,
                sound_wanted: queued.sound.is_some(),
                dnd,
            });
            if let (true, Some(kind)) = (plan.sound, queued.sound) {
                self.play_background_sound(account, kind, queued.chat_id);
            }
            self.show_background_notification(account, queued, cx);
        }
        for path in drain.sound_plays {
            self.play_sound_file(path);
        }
    }

    /// The account on screen, or the one about to be (no session yet).
    fn current_account(&self) -> Option<AccountKey> {
        self.session()
            .map(|session| session.account.clone())
            .or_else(|| safe_app_root().map(|root| active_account(&root)))
    }

    /// A click on a notification whose tag names another account than the
    /// one on screen (macOS and Windows callback).
    pub(in crate::ui) fn queue_account_click(
        &mut self,
        tag_account: &str,
        chat_id: ChatId,
        action: NotificationAction,
        cx: &mut Context<Self>,
    ) {
        let Some(active) = self.current_account() else {
            return;
        };
        match plan_click(tag_account, action, &active, &self.background.keys()) {
            ClickPlan::Ignore => return,
            ClickPlan::Active => {
                if let Ok(mut clicks) = self.notify.notify_clicks.lock() {
                    clicks.push((chat_id, action));
                }
                self.drain_background_clicks(cx);
            }
            ClickPlan::MarkReadInBackground(key) | ClickPlan::SwitchThenOpen(key) => {
                if let Ok(mut clicks) = self.background.clicks.lock() {
                    clicks.push((key, chat_id, action));
                }
                self.drain_account_clicks(cx);
            }
        }
        cx.notify();
    }

    /// Run the account clicks that need no window ("Mark as read" on the
    /// background account) and bring the app forward for the ones that
    /// switch accounts, which the next render runs
    /// ([`Self::flush_background_accounts`]). Called from the OS callback
    /// and from both polls (Linux clicks arrive on worker threads).
    pub(in crate::ui) fn drain_account_clicks(&mut self, cx: &mut Context<Self>) {
        let clicks: Vec<(AccountKey, ChatId, NotificationAction)> =
            self.background.clicks.lock_or_recover().drain(..).collect();
        if clicks.is_empty() {
            return;
        }
        let active = self.current_account();
        let mut switches = Vec::new();
        for (account, chat_id, action) in clicks {
            if active.as_ref() == Some(&account) {
                // Switched to that account since the notification showed.
                if let Ok(mut guard) = self.notify.notify_clicks.lock() {
                    guard.push((chat_id, action));
                }
                continue;
            }
            match action {
                NotificationAction::MarkRead => {
                    if let Some(live) = self
                        .background
                        .client_mut(&account)
                        .and_then(|client| client.live.as_mut())
                    {
                        let _ = live.driver.mark_chat_as_read(chat_id);
                    }
                    self.dismiss_notification_as(Some(&account), chat_id, cx);
                }
                NotificationAction::Open | NotificationAction::Reply => {
                    switches.push((account, chat_id, action));
                }
            }
        }
        self.drain_background_clicks(cx);
        if switches.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.background.clicks.lock() {
            guard.extend(switches);
        }
        // A hidden or minimized window may never render on its own.
        cx.activate(true);
        for window in cx.windows() {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        cx.notify();
    }

    /// Render-time part (needs the window): the taskbar flash a background
    /// notification asked for, account switches asked for by clicks, and
    /// the chat a switch is waiting to open.
    pub(in crate::ui) fn flush_background_accounts(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if std::mem::take(&mut self.background.attention)
            && plan_alert(AlertInput {
                flash_enabled: true,
                sound_wanted: false,
                dnd: dnd_active(),
            })
            .flash
        {
            window.request_attention();
        }
        let clicks: Vec<(AccountKey, ChatId, NotificationAction)> =
            self.background.clicks.lock_or_recover().drain(..).collect();
        let mut later = Vec::new();
        for (account, chat_id, action) in clicks {
            if action == NotificationAction::MarkRead {
                later.push((account, chat_id, action));
                continue;
            }
            if self.current_account().as_ref() != Some(&account) {
                self.switch_account(&account, "", window, cx);
            }
            self.background.pending_open = Some(PendingOpen {
                account,
                chat_id,
                action,
                until: Instant::now() + PENDING_OPEN_FOR,
            });
        }
        if !later.is_empty()
            && let Ok(mut guard) = self.background.clicks.lock()
        {
            guard.extend(later);
        }
        self.open_pending_account_chat(window, cx);
        self.flush_background_call_picks(window, cx);
    }

    /// Open the chat a notification switched accounts for, once that
    /// account's session is ready and knows the chat.
    fn open_pending_account_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.background.pending_open.as_ref() else {
            return;
        };
        if Instant::now() > pending.until {
            self.background.pending_open = None;
            return;
        }
        let ready = match self.session() {
            // Still connecting: no session yet.
            None => return,
            Some(session) if session.account != pending.account => {
                // The switch did not happen (or another one followed).
                self.background.pending_open = None;
                return;
            }
            Some(session) => {
                matches!(session.auth, AuthorizationState::Ready)
                    && session.chats.contains_key(&pending.chat_id.0)
            }
        };
        if !ready {
            return;
        }
        if let Some(pending) = self.background.pending_open.take() {
            self.run_notification_action(pending.chat_id, pending.action, window, cx);
        }
    }
}
