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
use quill::settings::{
    ChatWindowsState, RestoreAnswer, SavedChatWindow, load_chat_windows, save_chat_windows,
};

/// Where the windows from last time stand.
#[derive(Debug, Default)]
pub(super) enum Restore {
    /// Not looked at yet (the account was not ready).
    #[default]
    Unlooked,
    /// Waiting for the answer to the offer, holding what it would restore.
    Offered(Vec<SavedChatWindow>),
    /// Windows still to open as their chats load; the rest is dropped once the
    /// chat list is complete. `refused` are the ones whose open was
    /// refused: they are not tried again this launch and stay in the file.
    ToOpen {
        waiting: Vec<SavedChatWindow>,
        refused: Vec<SavedChatWindow>,
    },
    /// Nothing more happens this launch. These windows were not brought
    /// back (their open was refused, or the offer was dismissed unanswered)
    /// and stay in the file for the next launch.
    Done(Vec<SavedChatWindow>),
}

impl Restore {
    /// The offer's box was closed with no answer: "not now". The windows
    /// stay in the file; any other stage is unchanged.
    fn dismissed(self) -> Self {
        match self {
            Self::Offered(saved) => Self::Done(saved),
            other => other,
        }
    }
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
    (state.answer == RestoreAnswer::Yes).then(|| state.windows_of(account))
}

/// Split saved windows into those whose chat has loaded (as chat and
/// geometry, ready to open) and those still waiting for their chat.
pub(super) fn split_ready(
    windows: Vec<SavedChatWindow>,
    chat_exists: impl Fn(ChatId) -> bool,
) -> (Vec<ReadyWindow>, Vec<SavedChatWindow>) {
    let (ready, waiting): (Vec<_>, Vec<_>) = windows
        .into_iter()
        .partition(|window| chat_exists(ChatId(window.chat_id)));
    let ready = ready
        .into_iter()
        .map(|window| {
            let place = (ChatId(window.chat_id), bounds_of(&window));
            (window, place.0, place.1)
        })
        .collect();
    (ready, waiting)
}

/// A saved window whose chat has loaded: the saved entry (kept in the file
/// when its open is refused), its chat and its geometry.
pub(super) type ReadyWindow = (SavedChatWindow, ChatId, Bounds<Pixels>);

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
        Restore::Unlooked => return None,
        Restore::Offered(saved) | Restore::Done(saved) => saved.clone(),
        Restore::ToOpen { waiting, refused } => waiting.iter().chain(refused).cloned().collect(),
    };
    let mut all = open;
    for window in not_back {
        if !all.iter().any(|open| open.chat_id == window.chat_id) {
            all.push(window);
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
            .map(|open| (open.chat_id, open.handle))
            .collect();
        // A change still waiting for its timer is saved now too: the timer
        // is the screen's and goes with it, so a window closed just before
        // the switch would otherwise come back next time.
        if !windows.is_empty() || self.windows.save.due_on(self.screen_epoch()) {
            self.save_chat_windows_now(cx);
        }
        self.windows.save = SaveTimer::Idle;
        for (chat_id, handle) in windows {
            self.in_window_scope(Some(chat_id), cx, |app, cx| app.flush_leaving_draft(cx));
            self.forget_chat_window(chat_id, cx);
            // The switch may run inside this window's own update, which
            // holds it: the removal waits until it is let go.
            // screen-scope: the window is already forgotten; this only closes the OS window.
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
        self.windows.restore = Restore::Unlooked;
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
            self.windows.restore = Restore::Done(Vec::new());
            return;
        };
        match std::mem::take(&mut self.windows.restore) {
            Restore::Unlooked => {
                let state = load_chat_windows();
                let saved = state.windows_of(&account);
                self.windows.restore = if saved.is_empty() {
                    Restore::Done(Vec::new())
                } else if state.answer == RestoreAnswer::Unasked {
                    self.offer_restore(window, cx);
                    Restore::Offered(saved)
                } else {
                    match to_restore(&state, &account) {
                        Some(waiting) => Restore::ToOpen {
                            waiting,
                            refused: Vec::new(),
                        },
                        None => Restore::Done(Vec::new()),
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
            Restore::ToOpen {
                waiting,
                mut refused,
            } => {
                let (ready, waiting) = split_ready(waiting, |chat| {
                    self.session()
                        .is_some_and(|s| s.chats.contains_key(&chat.0))
                });
                // A window whose open was refused (the driver, say) is not
                // lost: it stays in the file for the next launch. One whose
                // chat can never have a window is dropped for good.
                for (saved, chat_id, bounds) in ready {
                    let openable = self
                        .session()
                        .and_then(|s| s.chats.get(&chat_id.0))
                        .is_some_and(|chat| self.can_open_chat_window(chat));
                    if openable && !self.try_open_chat_window_at(chat_id, Some(bounds), window, cx)
                    {
                        refused.push(saved);
                    }
                }
                // Chats still loading get their windows when they arrive;
                // once the list is complete the rest is gone for good.
                let exhausted = self
                    .session()
                    .is_some_and(|s| s.chat_list.main_paging.is_exhausted());
                self.windows.restore = if waiting.is_empty() || exhausted {
                    Restore::Done(refused)
                } else {
                    Restore::ToOpen { waiting, refused }
                };
            }
            Restore::Done(kept) => self.windows.restore = Restore::Done(kept),
        }
    }

    /// Telegram Desktop's one-time question (`lng_restore_windows_*`).
    fn offer_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let yes = app.clone();
            let no = app.clone();
            let closed = app.clone();
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
                // After an answer too, when the stage has left the offer
                // and this does nothing.
                .on_close(move |_, _, cx| {
                    let _ = closed.update(cx, |this, cx| this.dismiss_restore(cx));
                })
        });
    }

    /// The answer to the offer, kept for every later launch.
    pub(super) fn answer_restore(&mut self, restore: bool, cx: &mut Context<Self>) {
        let mut state = load_chat_windows();
        state.answer(restore);
        save_chat_windows(&state);
        self.windows.restore = match std::mem::take(&mut self.windows.restore) {
            Restore::Offered(waiting) if restore => Restore::ToOpen {
                waiting,
                refused: Vec::new(),
            },
            _ => Restore::Done(Vec::new()),
        };
        cx.notify();
    }

    /// The offer's box was closed with no answer (Escape, a click away):
    /// "not now". The windows stay in the file, nothing is asked again this
    /// launch, and no answer is stored, so the next launch asks again.
    fn dismiss_restore(&mut self, cx: &mut Context<Self>) {
        let offered = matches!(self.windows.restore, Restore::Offered(_));
        self.windows.restore = std::mem::take(&mut self.windows.restore).dismissed();
        if offered {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    // Not a glob: `gpui_kit::*` would shadow the built-in `#[test]`.
    use super::{Restore, split_ready, to_restore, windows_to_save};
    use gpui_kit::{point, px, size};
    use quill::ids::ChatId;
    use quill::settings::{ChatWindowsState, RestoreAnswer, SavedChatWindow};

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
            answer: RestoreAnswer::Unasked,
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
        assert_eq!(ready[0].1, ChatId(1));
        assert_eq!(ready[0].2.origin, point(px(10.), px(20.)));
        assert_eq!(ready[0].2.size, size(px(700.), px(500.)));
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].chat_id, 2);
    }

    #[test]
    fn a_dismissed_offer_keeps_its_windows_and_a_settled_stage_is_left_alone() {
        let kept = |restore: Restore| match restore {
            Restore::Done(saved) => Some(saved.len()),
            _ => None,
        };
        assert_eq!(kept(Restore::Offered(vec![saved(1)]).dismissed()), Some(1));
        // The box closes after an answer too: that changes nothing.
        let opening = Restore::ToOpen {
            waiting: vec![saved(1)],
            refused: Vec::new(),
        };
        assert!(matches!(opening.dismissed(), Restore::ToOpen { .. }));
        assert_eq!(kept(Restore::Done(vec![saved(2)]).dismissed()), Some(1));
    }

    #[test]
    fn a_save_in_the_middle_of_a_restore_keeps_the_windows_not_back_yet() {
        let chats = |windows: Option<Vec<SavedChatWindow>>| {
            windows.map(|all| all.iter().map(|w| w.chat_id).collect::<Vec<_>>())
        };
        let waiting = vec![saved(1), saved(2), saved(3)];
        // Chat 1 is open; 2 and 3 still wait for their chats.
        let opening = Restore::ToOpen {
            waiting: waiting.clone(),
            refused: Vec::new(),
        };
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
        assert_eq!(windows_to_save(vec![saved(1)], &Restore::Unlooked), None);
        // Done with nothing kept: what is open is what is saved.
        assert_eq!(
            chats(windows_to_save(vec![saved(1)], &Restore::Done(Vec::new()))),
            Some(vec![1])
        );
        // Done with a refused or dismissed window: it stays in the file.
        assert_eq!(
            chats(windows_to_save(
                vec![saved(1)],
                &Restore::Done(vec![saved(2)])
            )),
            Some(vec![1, 2])
        );
    }
}
