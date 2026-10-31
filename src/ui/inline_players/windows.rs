//! Which windows show each clip (`None`: the main window, else the chat
//! window showing that chat). Every window renders its history and its
//! panels in passes of its own, so a clip leaves a window when its row did
//! not render in that window's previous pass of its list, when the
//! window's frame drew no history, when a viewer covers the window's chat,
//! when a panel closes there, or when the window closes. Its player goes
//! once no window shows it. (One pass count for every window let each
//! window's pass drop the clips only the other one showed, so a chat
//! window's clips were reopened every frame while the main window drew.)
#![deny(clippy::indexing_slicing)]

use super::{InlineVideos, Key, Panel, Slot, State};
use quill::ids::ChatId;
use std::collections::HashMap;

/// Where a slot's row renders: the history, or a panel's grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum List {
    History,
    Panel(Panel),
}

impl List {
    pub(super) fn of(key: Key) -> Self {
        Panel::of(key.0).map_or(Self::History, Self::Panel)
    }
}

/// What one window shows of a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Seen {
    /// The pass of the slot's list there its row last rendered in.
    pub(super) pass: u64,
    /// That window's last sweep found the picture painted in view.
    pub(super) in_view: bool,
}

/// The windows a slot shows in.
pub(super) type Windows = HashMap<Option<ChatId>, Seen>;

/// A render pass of one window's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pass {
    pub(super) window: Option<ChatId>,
    pub(super) list: List,
    pub(super) number: u64,
}

/// As `pass` begins: the slot for `key` leaves the pass's window when its
/// row did not render in that window's previous pass of its list, and
/// stays while any window still shows it.
pub(super) fn survives(key: Key, windows: &mut Windows, pass: Pass) -> bool {
    if List::of(key) == pass.list {
        windows.retain(|window, seen| {
            *window != pass.window || seen.pass.saturating_add(1) >= pass.number
        });
    }
    !windows.is_empty()
}

/// Which players a sweep judges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sweep {
    /// Those the history pass of this window just painted.
    Pass(Option<ChatId>),
    /// Every one (the release timer).
    Timer,
}

/// Record what a sweep found of a slot (`painted`: its picture was painted
/// in view since the pass mark) and return whether any window has it in
/// view now. A pass's sweep speaks for its window; the timer's, which
/// cannot tell which window painted it, for every one.
pub(super) fn note_sweep(windows: &mut Windows, sweep: Sweep, painted: bool) -> bool {
    for (window, seen) in windows.iter_mut() {
        if sweep == Sweep::Timer || sweep == Sweep::Pass(*window) {
            seen.in_view = painted;
        }
    }
    windows.values().any(|seen| seen.in_view)
}

impl State {
    /// The number of `window`'s current pass of `list`.
    pub(super) fn pass(&self, window: Option<ChatId>, list: List) -> u64 {
        self.passes.get(&(window, list)).copied().unwrap_or(0)
    }

    /// Begin `window`'s next pass of `list`: the slots whose rows did not
    /// render in its last one leave it. Returns the slots no window shows
    /// any more, to drop once the borrow is released.
    pub(super) fn begin_pass(&mut self, window: Option<ChatId>, list: List) -> HashMap<Key, Slot> {
        let number = self.pass(window, list).saturating_add(1);
        self.passes.insert((window, list), number);
        let pass = Pass {
            window,
            list,
            number,
        };
        self.retire(|key, windows| survives(key, windows, pass))
    }

    /// The slots of `list` (`None`: of every list) leave `window`.
    fn leave(&mut self, window: Option<ChatId>, list: Option<List>) -> HashMap<Key, Slot> {
        self.retire(|key, windows| {
            if list.is_none_or(|list| list == List::of(key)) {
                windows.remove(&window);
            }
            !windows.is_empty()
        })
    }

    /// Take out the slots no window shows once `stays` (which takes
    /// windows off a slot) has seen each, and forget their demands.
    fn retire(&mut self, mut stays: impl FnMut(Key, &mut Windows) -> bool) -> HashMap<Key, Slot> {
        let gone: HashMap<Key, Slot> = self
            .players
            .extract_if(|key, slot| !stays(*key, &mut slot.windows))
            .collect();
        self.forget(&gone);
        gone
    }
}

impl InlineVideos {
    /// Start a render pass of `panel` in `window`: its clips whose cells
    /// did not render in the last one there (results that left the panel)
    /// leave it. Called at the top of the panel's render.
    pub(in crate::ui) fn begin_panel_render(&self, window: Option<ChatId>, panel: Panel) {
        let dropped = self.with(|state| state.begin_pass(window, List::Panel(panel)));
        drop(dropped);
    }

    /// Called at the start of every frame of `window`, before anything in
    /// it renders: when its last frame drew no history pass where one was
    /// due (`history_drawn`; no chat open, Contacts, Calls…), nothing there
    /// can show a clip, so its history's clips leave it.
    pub(in crate::ui) fn frame_start(&self, window: Option<ChatId>, history_drawn: bool) {
        let dropped = self.with(|state| {
            let swept = state.swept.remove(&window);
            if history_drawn && !swept {
                state.leave(window, Some(List::History))
            } else {
                HashMap::new()
            }
        });
        drop(dropped);
    }

    /// A viewer covers `window`'s chat: every clip leaves it, and the
    /// demands no other window carries out go.
    pub(in crate::ui) fn leave_window(&self, window: Option<ChatId>) {
        let dropped = self.with(|state| {
            let dropped = state.leave(window, None);
            let players = &state.players;
            state.on_demand.retain(|key, _| players.contains_key(key));
            dropped
        });
        drop(dropped);
    }

    /// `panel` is closed in `window`: its clips leave it.
    pub(in crate::ui) fn leave_panel(&self, window: Option<ChatId>, panel: Panel) {
        let list = List::Panel(panel);
        let dropped = self.with(|state| {
            if !state.players.keys().any(|key| List::of(*key) == list) {
                return HashMap::new();
            }
            state.leave(window, Some(list))
        });
        drop(dropped);
    }

    /// The chat window showing `chat` closed: its clips leave it.
    pub(in crate::ui) fn forget_window(&self, chat: ChatId) {
        let window = Some(chat);
        let dropped = self.with(|state| {
            state.passes.retain(|(on, _), _| *on != window);
            state.views.remove(&window);
            state.swept.remove(&window);
            state.leave(window, None)
        });
        drop(dropped);
    }
}

#[cfg(test)]
mod tests {
    use super::{List, Panel, Pass, Seen, Sweep, Windows, note_sweep, survives};
    use quill::ids::ChatId;

    const MAIN: Option<ChatId> = None;
    const CHAT_WINDOW: Option<ChatId> = Some(ChatId(11));

    /// A slot shown in each of `windows` since that window's pass.
    fn shown(windows: &[(Option<ChatId>, u64)]) -> Windows {
        windows
            .iter()
            .map(|(window, pass)| {
                (
                    *window,
                    Seen {
                        pass: *pass,
                        in_view: false,
                    },
                )
            })
            .collect()
    }

    fn pass(window: Option<ChatId>, list: List, number: u64) -> Pass {
        Pass {
            window,
            list,
            number,
        }
    }

    #[test]
    fn history_and_panel_passes_each_reap_only_their_own_players() {
        let row = (11, 204);
        let cell = (Panel::InlineResults.chat(), 7);
        let gif = (Panel::Gifs.chat(), 7);
        let stays = |key, seen: u64, at: Pass| survives(key, &mut shown(&[(MAIN, seen)]), at);
        // A history pass keeps rows seen in the previous one, and every
        // panel cell however old.
        let history = |number| pass(MAIN, List::History, number);
        assert!(stays(row, 4, history(5)));
        assert!(!stays(row, 3, history(5)));
        assert!(stays(cell, 0, history(500)));
        assert!(stays(gif, 0, history(500)));
        // A panel pass reaps its cells that left the panel, never history
        // rows or the other panel's cells.
        let inline = |number| pass(MAIN, List::Panel(Panel::InlineResults), number);
        assert!(stays(cell, 2, inline(3)));
        assert!(!stays(cell, 1, inline(3)));
        assert!(stays(row, 0, inline(500)));
        assert!(stays(gif, 0, inline(500)));
        assert!(!stays(gif, 1, pass(MAIN, List::Panel(Panel::Gifs), 3)));
    }

    #[test]
    fn a_windows_pass_leaves_alone_the_clips_only_another_window_shows() {
        let row = (11, 611);
        // The main window passes many times while a chat window shows the
        // clip: it stays, however old the chat window's pass count is.
        let mut windows = shown(&[(CHAT_WINDOW, 4)]);
        for number in 1..100 {
            assert!(survives(
                row,
                &mut windows,
                pass(MAIN, List::History, number)
            ));
        }
        assert_eq!(windows, shown(&[(CHAT_WINDOW, 4)]));
        // The chat window's own next pass without its row: it goes.
        assert!(!survives(
            row,
            &mut windows,
            pass(CHAT_WINDOW, List::History, 6)
        ));
    }

    #[test]
    fn a_clip_both_windows_show_stays_until_the_last_one_stops() {
        let row = (11, 611);
        let mut windows = shown(&[(MAIN, 3), (CHAT_WINDOW, 9)]);
        // The main window's row went: the clip leaves it, not the other.
        assert!(survives(row, &mut windows, pass(MAIN, List::History, 5)));
        assert_eq!(windows, shown(&[(CHAT_WINDOW, 9)]));
        assert!(survives(
            row,
            &mut windows,
            pass(CHAT_WINDOW, List::History, 10)
        ));
        assert!(!survives(
            row,
            &mut windows,
            pass(CHAT_WINDOW, List::History, 11)
        ));
    }

    #[test]
    fn a_clip_stays_in_view_while_any_window_shows_it_in_view() {
        let mut windows = shown(&[(MAIN, 1), (CHAT_WINDOW, 1)]);
        // In view in the chat window, off screen (overdraw) in the main one.
        assert!(note_sweep(&mut windows, Sweep::Pass(CHAT_WINDOW), true));
        assert!(note_sweep(&mut windows, Sweep::Pass(MAIN), false));
        // Scrolled away in the chat window too: off screen everywhere.
        assert!(!note_sweep(&mut windows, Sweep::Pass(CHAT_WINDOW), false));
        // The timer judges for every window.
        assert!(note_sweep(&mut windows, Sweep::Timer, true));
        assert!(!note_sweep(&mut windows, Sweep::Timer, false));
    }
}
