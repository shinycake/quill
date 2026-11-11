//! Background accounts' notifications reaching the screen, and what a click
//! on one does: "Mark as read" reads the chat on that account in place;
//! Open and Reply switch to the account and then open the chat (tdesktop
//! `Window::Notifications::Manager::openNotificationMessage` activates the
//! notification's account first).

use super::*;
use quill::locks::LockExt;
use quill::notify_accounts::{ChatClick, ClickPlan, plan_account_click};
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
    pub(in crate::ui) fn current_account(&self) -> Option<AccountKey> {
        self.session()
            .map(|session| session.account.clone())
            .or_else(|| safe_app_root().map(|root| active_account(&root)))
    }

    /// A click on a notification, from the macOS and Windows callback or
    /// Quill's own notification windows: the account on screen runs it,
    /// another account reads in place or switches first.
    pub(in crate::ui) fn queue_account_click(&mut self, click: ChatClick, cx: &mut Context<Self>) {
        let Some(active) = self.current_account() else {
            return;
        };
        match plan_account_click(
            &click.account,
            click.action,
            Some(&active),
            &self.background.keys(),
        ) {
            ClickPlan::Ignore => return,
            ClickPlan::Active => {
                if let Ok(mut clicks) = self.delivery.notify_clicks.lock() {
                    clicks.push(click);
                }
                self.drain_background_clicks(cx);
            }
            ClickPlan::MarkReadInBackground(_) | ClickPlan::SwitchThenOpen(_) => {
                if let Ok(mut clicks) = self.background.clicks.lock() {
                    clicks.push(click);
                }
                self.drain_account_clicks(cx);
            }
        }
        cx.notify();
    }

    /// Run the account clicks that need no window ("Mark as read" on the
    /// background account) and bring the app forward for the ones that
    /// switch accounts, which the next render runs
    /// ([`Self::flush_background_accounts`]). The account on screen's
    /// clicks move to `notify.notify_clicks`. Called from the OS callback
    /// and from both polls (Linux clicks arrive on worker threads).
    pub(in crate::ui) fn drain_account_clicks(&mut self, cx: &mut Context<Self>) {
        let clicks: Vec<ChatClick> = self.background.clicks.lock_or_recover().drain(..).collect();
        if clicks.is_empty() {
            return;
        }
        let active = self.current_account();
        let known = self.background.keys();
        let mut switches = Vec::new();
        // Read receipts wait for the unlock, queued.
        let mut held = Vec::new();
        for click in clicks {
            match plan_account_click(&click.account, click.action, active.as_ref(), &known) {
                // Shown for the account on screen, or switched to it since.
                ClickPlan::Active => {
                    if let Ok(mut guard) = self.delivery.notify_clicks.lock() {
                        guard.push(click);
                    }
                }
                ClickPlan::MarkReadInBackground(_) if self.refused_while_locked() => {
                    held.push(click);
                }
                ClickPlan::MarkReadInBackground(account) => {
                    if let Some(live) = self
                        .background
                        .client_mut(&account)
                        .and_then(|client| client.live.as_mut())
                    {
                        let _ = live.driver.mark_chat_as_read(click.chat_id);
                    }
                    self.dismiss_notification_as(Some(&account), click.chat_id, cx);
                }
                ClickPlan::SwitchThenOpen(_) => switches.push(click),
                // An account Quill no longer runs: never the one on screen.
                ClickPlan::Ignore => {}
            }
        }
        self.drain_background_clicks(cx);
        if !held.is_empty() {
            self.background.clicks.lock_or_recover().extend(held);
        }
        if switches.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.background.clicks.lock() {
            guard.extend(switches);
        }
        // A hidden or minimized window may never render on its own.
        crate::ui::notifications::activate_everything(&self.delivery.windows.window_ids(), cx);
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
        let clicks: Vec<ChatClick> = self.background.clicks.lock_or_recover().drain(..).collect();
        let mut later = Vec::new();
        let mut on_screen = false;
        for click in clicks {
            let plan = plan_account_click(
                &click.account,
                click.action,
                self.current_account().as_ref(),
                &self.background.keys(),
            );
            match plan {
                ClickPlan::Active => {
                    // The Linux threads queue every click here; the account
                    // on screen runs its own the usual way.
                    if let Ok(mut guard) = self.delivery.notify_clicks.lock() {
                        guard.push(click);
                    }
                    on_screen = true;
                    continue;
                }
                ClickPlan::MarkReadInBackground(_) => {
                    later.push(click);
                    continue;
                }
                ClickPlan::Ignore => continue,
                // tdesktop's `openNotificationMessage` only shows the
                // window while locked; the click has done that already.
                ClickPlan::SwitchThenOpen(_) if self.refused_while_locked() => continue,
                ClickPlan::SwitchThenOpen(_) => {}
            }
            self.switch_account(&click.account, "", window, cx);
            self.background.pending_open = Some(PendingOpen {
                account: click.account,
                chat_id: click.chat_id,
                action: click.action,
                until: Instant::now() + PENDING_OPEN_FOR,
            });
        }
        if !later.is_empty()
            && let Ok(mut guard) = self.background.clicks.lock()
        {
            guard.extend(later);
        }
        if on_screen {
            self.drain_background_clicks(cx);
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
