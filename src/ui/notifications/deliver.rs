//! Handing out what the account on screen queued for the OS, from the poll
//! (`QuillApp::poll_live`) or the main window's render.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::audio::NotificationSound;
use gpui_kit::*;
use quill::locks::LockExt;

impl QuillApp {
    /// Hand out what the account on screen queued for the OS: withdrawn
    /// notifications, banners, sounds, the Dock bounce, the incoming-call
    /// toast and ringtone, and a settled tone upload. Each poll runs this, because a
    /// hidden, minimized or covered main window does not draw (tdesktop's
    /// notification manager runs on data events too). `window`: the main
    /// window, when its render runs this.
    pub(super) fn deliver_notifications(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if let Some(drain) = self
            .live
            .as_mut()
            .map(|live| quill::notify_accounts::drain_on_screen(&mut live.driver.session))
        {
            self.hand_out_notifications(drain, window, cx);
        }
        // The call windows follow the call from here too, not from the
        // main window's frame alone, which a hidden main window never draws.
        self.sync_call_windows(cx);
        self.announce_ringing_call(cx);
        // The ringtone follows the call's state, as tdesktop's
        // `Calls::Instance` does, not a paint; the call tick keeps it looping.
        self.ensure_call_tick(cx);
        self.sync_call_sounds();
        self.settle_tone_upload(cx);
    }

    /// Show one drain of the account on screen (`deliver_notifications`).
    pub(in crate::ui) fn hand_out_notifications(
        &mut self,
        drain: quill::notify_accounts::NotifyDrain,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        // Notifications whose chat was read elsewhere (or removed by TDLib)
        // are withdrawn from the OS notification center.
        for chat_id in drain.clears {
            self.dismiss_os_notification(chat_id, cx);
        }
        // tdesktop skips its own sound and the Dock bounce / taskbar flash
        // while the system is in Do Not Disturb; the OS banner is the OS's.
        // Only probe the OS when there is something to decide about.
        let dnd = quill::notify_focus::needs_probe(drain.attention, drain.notifications.len())
            && quill::notify_focus::dnd_active();
        if drain.attention
            && quill::notify_focus::plan_alert(quill::notify_focus::AlertInput {
                flash_enabled: true,
                sound_wanted: false,
                dnd,
            })
            .flash
        {
            match window {
                Some(window) => window.request_attention(),
                None => {
                    if let Some(main) = self.main_window.handle {
                        let _ = main.update(cx, |_, window, _| window.request_attention());
                    }
                }
            }
        }
        for queued in drain.notifications {
            let plan = quill::notify_focus::plan_alert(quill::notify_focus::AlertInput {
                flash_enabled: false,
                sound_wanted: queued.sound.is_some(),
                dnd,
            });
            if let (true, Some(kind)) = (plan.sound, queued.sound) {
                self.play_notification_sound(kind, queued.chat_id);
            }
            self.spawn_os_notification(queued, cx);
        }
        // Parity slice: custom sounds whose downloads just completed.
        for path in drain.sound_plays {
            self.delivery
                .notification_sounds
                .play(NotificationSound::File(path));
        }
    }

    /// The incoming call ringing on the account on screen: `(call id,
    /// caller user id, video)`.
    fn ringing_call(&self) -> Option<(i32, i64, bool)> {
        self.session()
            .and_then(quill::notify_accounts::ringing_incoming_call)
    }

    /// Picks on the incoming-call notification. Accepting must never act
    /// on a different call, and TDLib call ids repeat across accounts, so
    /// only the picks for the call ringing on screen are answered here. A
    /// pick for a call still ringing on a background account stays queued
    /// for `flush_background_call_picks`; any other (the call already ended
    /// or was answered) is dropped.
    pub(in crate::ui) fn answer_call_picks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ringing = self
            .session()
            .map(|session| session.account.clone())
            .zip(self.ringing_call().map(|(call_id, ..)| call_id));
        let queue = self.calls.notify_clicks.clone();
        let mine = {
            let mut picks = queue.lock_or_recover();
            let mine = match &ringing {
                Some((account, call_id)) => {
                    quill::notify_call::take_call_picks(&mut picks, account, *call_id)
                }
                None => Vec::new(),
            };
            picks.retain(|pick| self.background_call_rings(pick));
            mine
        };
        for action in mine {
            match action {
                quill::notify_call::CallNotificationAction::Accept => self.accept_incoming_call(cx),
                quill::notify_call::CallNotificationAction::Decline => self.hang_up_call(cx),
                quill::notify_call::CallNotificationAction::Open => {
                    self.call_screen.window_closed_by_user = None;
                    cx.activate(true);
                    window.activate_window();
                    if let Some(handle) = self.calls.window {
                        let _ = handle.update(cx, |_, window, _| window.activate_window());
                    }
                }
            }
        }
    }

    /// Whether the window in front shows an incoming call: the main
    /// window (its call bar) or the call window itself.
    fn call_ring_seen(&self, cx: &App) -> bool {
        cx.active_window().is_some_and(|active| {
            Some(active) == self.main_window.handle || Some(active) == self.calls.window
        })
    }

    /// Announce a new ringing call when no Quill window is in front, and
    /// withdraw the notification once the call stops ringing.
    fn announce_ringing_call(&mut self, cx: &mut Context<Self>) {
        let Some((call_id, user_id, is_video)) = self.ringing_call() else {
            // Not ringing any more: take the toast down (macOS and Windows;
            // a Linux `notify-send` toast expires on its own).
            if let Some((account, call_id)) = self.calls.notified.take()
                && quill::notify::current_backend() == quill::notify::NotifyBackend::Native
            {
                cx.dismiss_system_notification(&quill::notify_call::call_notification_tag(
                    &account.0, call_id,
                ));
            }
            return;
        };
        let account = self.notification_account(None);
        let caller = self.call_peer_name(user_id);
        let Some(notification) =
            quill::notify_call::decide_call_notify(&quill::notify_call::CallNotifyInput {
                call_id,
                caller: &caller,
                is_video,
                // Seen in the main or the call window only: a chat window
                // in front shows no call, so the OS notification goes out.
                app_active: self.call_ring_seen(cx),
                // Call ids are per account: another account's call with
                // the same id was a different call.
                last_notified: self
                    .calls
                    .notified
                    .as_ref()
                    .filter(|(notified_on, _)| *notified_on == account)
                    .map(|(_, call_id)| *call_id),
                locked: self.account.passcode.is_locked(),
            })
        else {
            return;
        };
        self.calls.notified = Some((account.clone(), call_id));
        self.spawn_call_notification_as(&account, notification, cx);
    }
}
