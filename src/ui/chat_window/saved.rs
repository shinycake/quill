//! Chat windows across launches (Telegram Desktop `Window::SavedWindows`):
//! the open chat windows and their geometry are saved as they change;
//! at the next launch, once the account is ready, the user is asked once
//! whether to restore them, and from then on the answer decides.

use super::super::app::QuillApp;
use super::super::on_screen::ScreenEpoch;
use gpui_kit::component::WindowExt;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::settings::{ChatWindowsState, SavedChatWindow, load_chat_windows, save_chat_windows};

/// Where the windows from last time stand.
#[derive(Debug, Default)]
pub(super) enum Restore {
    /// Not looked at yet (the account was not ready).
    #[default]
    Pending,
    /// Waiting for the answer to the offer, holding what it would restore.
    Offered(Vec<SavedChatWindow>),
    /// Windows to open as their chats load; the rest is dropped once the
    /// chat list is complete.
    Opening(Vec<SavedChatWindow>),
    Done,
}

/// The write of the window places: nothing due, or a quiet-period timer
/// running for the screen it was started on. The timer is the stage, so
/// it goes with its owner; one started on an earlier screen epoch never
/// fires (its updates are dropped) and no longer counts as due.
#[derive(Debug, Default)]
pub(super) enum SaveTimer {
    #[default]
    Idle,
    Due(ScreenEpoch, Task<()>),
}

impl SaveTimer {
    pub(super) fn due_on(&self, screen: ScreenEpoch) -> bool {
        matches!(self, SaveTimer::Due(epoch, _) if *epoch == screen)
    }
}

/// The saved windows of `account` to bring back, when the answer was to
/// restore (`None` otherwise, or when the question is still open).
pub(super) fn to_restore(state: &ChatWindowsState, account: &str) -> Option<Vec<SavedChatWindow>> {
    (state.asked && state.restore).then(|| state.windows_of(account))
}

/// Split saved windows into those whose chat has loaded (as chat and
/// geometry, ready to open) and those still waiting for their chat.
pub(super) fn split_ready(
    windows: Vec<SavedChatWindow>,
    chat_exists: impl Fn(ChatId) -> bool,
) -> (Vec<(ChatId, Bounds<Pixels>)>, Vec<SavedChatWindow>) {
    let (ready, waiting): (Vec<_>, Vec<_>) = windows
        .into_iter()
        .partition(|window| chat_exists(ChatId(window.chat_id)));
    let ready = ready
        .iter()
        .map(|window| (ChatId(window.chat_id), bounds_of(window)))
        .collect();
    (ready, waiting)
}

/// What to write as the account's saved windows: the windows open now, plus
/// the saved ones a restore has not brought back yet (still waiting for
/// their chats, or for the answer to the offer), so a save in the middle of
/// a restore never forgets the windows that have not opened. `None` while
/// nothing was looked at yet: the file stays as it is.
pub(super) fn windows_to_save(
    open: Vec<SavedChatWindow>,
    restore: &Restore,
) -> Option<Vec<SavedChatWindow>> {
    let not_back = match restore {
        Restore::Pending => return None,
        Restore::Offered(saved) | Restore::Opening(saved) => saved.as_slice(),
        Restore::Done => &[],
    };
    let mut all = open;
    for window in not_back {
        if !all.iter().any(|open| open.chat_id == window.chat_id) {
            all.push(window.clone());
        }
    }
    Some(all)
}

fn bounds_of(window: &SavedChatWindow) -> Bounds<Pixels> {
    Bounds {
        origin: point(px(window.x), px(window.y)),
        size: size(px(window.width), px(window.height)),
    }
}

impl QuillApp {
    /// The account key the saved windows are filed under (live sessions
    /// only: demos and tests never touch the file).
    fn saved_windows_account(&self) -> Option<String> {
        self.live
            .as_ref()
            .map(|live| live.driver.session.account.0.clone())
    }

    /// Account switch. A chat window draws the account on screen, so the
    /// outgoing account's windows close: their places are saved under that
    /// account first, and the incoming account's saved windows come back
    /// once it is ready (switching back reopens these). Telegram Desktop
    /// keeps each window on its own account instead; Quill's windows have
    /// no session of their own.
    pub(in crate::ui) fn close_chat_windows_for_switch(&mut self, cx: &mut Context<Self>) {
        let windows: Vec<_> = self
            .windows
            .open
            .iter()
            .map(|open| (open.chat_id, open.handle, open.view.clone()))
            .collect();
        // A change still waiting for its timer is saved now too: the timer
        // is the screen's and goes with it, so a window closed just before
        // the switch would otherwise come back next time.
        if !windows.is_empty() || self.windows.save.due_on(self.screen_epoch()) {
            self.save_chat_windows_now(cx);
        }
        self.windows.save = SaveTimer::Idle;
        for (chat_id, handle, view) in windows {
            self.in_window_scope(Some(chat_id), cx, |app, cx| app.flush_leaving_draft(cx));
            self.forget_chat_window(chat_id, cx);
            let _ = view.update(cx, |view, _| view.mark_closed());
            // The switch may run inside this window's own update, which
            // holds it: the removal waits until it is let go.
            // screen-scope: the window is already forgotten; this only closes the OS window.
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
        self.windows.restore = Restore::Pending;
    }

    /// Write the open chat windows after a short quiet period (a window
    /// opened, closed, moved or resized).
    pub(super) fn schedule_chat_windows_save(&mut self, cx: &mut Context<Self>) {
        if self.saved_windows_account().is_none() || self.windows.save.due_on(self.screen_epoch()) {
            return;
        }
        // The screen's own timer: after a switch it does not run, so it
        // cannot file the next account's (still empty) windows under that
        // account before they are restored. The switch saves the old
        // account's windows itself (`close_chat_windows_for_switch`).
        let timer = self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(600))
                .await;
            let _ = this.update(cx, |this, cx| {
                // The task ends with this update: let it finish alone.
                if let SaveTimer::Due(_, task) = std::mem::take(&mut this.windows.save) {
                    task.detach();
                }
                this.save_chat_windows_now(cx);
            });
        });
        self.windows.save = SaveTimer::Due(self.screen_epoch(), timer);
    }

    fn save_chat_windows_now(&mut self, _cx: &mut App) {
        let Some(account) = self.saved_windows_account() else {
            return;
        };
        let windows: Vec<SavedChatWindow> = self
            .windows
            .open_places()
            .filter_map(|(chat_id, bounds)| {
                SavedChatWindow {
                    account: account.clone(),
                    chat_id: chat_id.0,
                    x: f32::from(bounds.origin.x),
                    y: f32::from(bounds.origin.y),
                    width: f32::from(bounds.size.width),
                    height: f32::from(bounds.size.height),
                }
                .sanitized()
            })
            .collect();
        let Some(windows) = windows_to_save(windows, &self.windows.restore) else {
            return;
        };
        let mut state = load_chat_windows();
        state.set_windows_of(&account, windows);
        save_chat_windows(&state);
    }

    /// Once per session, when the account is ready: offer the windows from
    /// last time, or bring them back as their chats load. Runs from the
    /// main window's frame upkeep.
    pub(in crate::ui) fn restore_chat_windows(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pane_mode() != super::super::app::PaneMode::Ready {
            return;
        }
        let Some(account) = self.saved_windows_account() else {
            self.windows.restore = Restore::Done;
            return;
        };
        match std::mem::take(&mut self.windows.restore) {
            Restore::Pending => {
                let state = load_chat_windows();
                let saved = state.windows_of(&account);
                self.windows.restore = if saved.is_empty() {
                    Restore::Done
                } else if !state.asked {
                    self.offer_restore(window, cx);
                    Restore::Offered(saved)
                } else {
                    match to_restore(&state, &account) {
                        Some(saved) => Restore::Opening(saved),
                        None => Restore::Done,
                    }
                };
            }
            Restore::Offered(saved) => {
                // The question is still open while its box is. A box lost
                // without an answer (another dialog replaced it, all
                // dialogs closed) asks again.
                if !window.has_active_dialog(cx) {
                    self.offer_restore(window, cx);
                }
                self.windows.restore = Restore::Offered(saved);
            }
            Restore::Opening(saved) => {
                let (ready, waiting) = split_ready(saved, |chat| {
                    self.session()
                        .is_some_and(|s| s.chats.contains_key(&chat.0))
                });
                for (chat_id, bounds) in ready {
                    self.open_chat_window_at(chat_id, Some(bounds), window, cx);
                }
                // Chats still loading get their windows when they arrive;
                // once the list is complete the rest is gone for good.
                let exhausted = self.session().is_some_and(|s| s.chat_list.chats_exhausted);
                self.windows.restore = if waiting.is_empty() || exhausted {
                    Restore::Done
                } else {
                    Restore::Opening(waiting)
                };
            }
            Restore::Done => self.windows.restore = Restore::Done,
        }
    }

    /// Telegram Desktop's one-time question (`lng_restore_windows_*`).
    fn offer_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let yes = app.clone();
            let no = app.clone();
            alert
                .title(tr(keys::Q_MISC_RESTORE_CHAT_WINDOWS))
                .description(tr(keys::Q_MISC_QUILL_CAN_BRING_BACK_THE))
                .ok_text(tr(keys::RESTORE_WINDOW))
                .cancel_text(tr(keys::Q_MISC_NOT_NOW))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = yes.update(cx, |this, cx| this.answer_restore(true, cx));
                    true
                })
                .on_cancel(move |_, _, cx| {
                    let _ = no.update(cx, |this, cx| this.answer_restore(false, cx));
                    true
                })
        });
    }

    /// The answer to the offer, kept for every later launch.
    pub(super) fn answer_restore(&mut self, restore: bool, cx: &mut Context<Self>) {
        let mut state = load_chat_windows();
        state.answer(restore);
        save_chat_windows(&state);
        self.windows.restore = match std::mem::take(&mut self.windows.restore) {
            Restore::Offered(saved) if restore => Restore::Opening(saved),
            _ => Restore::Done,
        };
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    // Not a glob: `gpui_kit::*` would shadow the built-in `#[test]`.
    use super::{Restore, split_ready, to_restore, windows_to_save};
    use gpui_kit::{point, px, size};
    use quill::ids::ChatId;
    use quill::settings::{ChatWindowsState, SavedChatWindow};

    fn saved(chat_id: i64) -> SavedChatWindow {
        SavedChatWindow {
            account: "a".into(),
            chat_id,
            x: 10.,
            y: 20.,
            width: 700.,
            height: 500.,
        }
    }

    #[test]
    fn the_windows_come_back_only_after_a_yes_and_as_their_chats_load() {
        let mut state = ChatWindowsState {
            asked: false,
            restore: false,
            windows: vec![saved(1), saved(2)],
        };
        // Not asked yet, or declined: nothing comes back.
        assert!(to_restore(&state, "a").is_none());
        state.answer(false);
        assert!(to_restore(&state, "a").is_none());
        state.answer(true);
        let saved = to_restore(&state, "a").expect("restore");
        assert_eq!(saved.len(), 2);
        // Another account's windows are not this account's.
        assert_eq!(to_restore(&state, "b").map(|w| w.len()), Some(0));
        // Chat 1 has loaded, chat 2 has not: one opens now, one waits.
        let (ready, waiting) = split_ready(saved, |chat| chat == ChatId(1));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, ChatId(1));
        assert_eq!(ready[0].1.origin, point(px(10.), px(20.)));
        assert_eq!(ready[0].1.size, size(px(700.), px(500.)));
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].chat_id, 2);
    }

    #[test]
    fn a_save_in_the_middle_of_a_restore_keeps_the_windows_not_back_yet() {
        let chats = |windows: Option<Vec<SavedChatWindow>>| {
            windows.map(|all| all.iter().map(|w| w.chat_id).collect::<Vec<_>>())
        };
        let waiting = vec![saved(1), saved(2), saved(3)];
        // Chat 1 is open; 2 and 3 still wait for their chats.
        let opening = Restore::Opening(waiting.clone());
        assert_eq!(
            chats(windows_to_save(vec![saved(1)], &opening)),
            Some(vec![1, 2, 3])
        );
        // Not answered yet: the offered windows are not forgotten either.
        let offered = Restore::Offered(waiting);
        assert_eq!(
            chats(windows_to_save(Vec::new(), &offered)),
            Some(vec![1, 2, 3])
        );
        // Not looked at: the file stays as it is.
        assert_eq!(windows_to_save(vec![saved(1)], &Restore::Pending), None);
        // Done: what is open is what is saved.
        assert_eq!(
            chats(windows_to_save(vec![saved(1)], &Restore::Done)),
            Some(vec![1])
        );
    }
}
