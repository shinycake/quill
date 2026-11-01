//! Which windows show each clip (`None`: the main window, else the chat
//! window showing that chat). Every window renders its history and its
//! panels in passes of its own, so a clip leaves a window when its row did
//! not render in that window's previous pass of its list, when the
//! window's frame drew no history, when a viewer covers the window's chat,
//! when a panel closes there, or when the window closes. Its player goes
//! once no window shows it. (One pass count for every window let each
//! window's pass drop the clips only the other one showed, so a chat
//! window's clips were reopened every frame while the main window drew.)
//!
//! A window the platform does not present (minimized, covered by another
//! app, on another Space), or the main window while the active chat window
//! hides it whole (`chat_window::cover`, which macOS does not report), has
//! none of its clips in view while another window is presented:
//! they pause, and sleep after [`super::RELEASE_AFTER`] like clips scrolled
//! away, until it is presented again. With no window presented, or with
//! one window, nothing changes.
#![deny(clippy::indexing_slicing)]

use super::{InlineVideos, Key, Panel, Slot, State};
use gpui_kit::{App, Window};
use quill::bubble_layout::HistoryWidth;
use quill::ids::ChatId;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

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
    /// The player's paint count (`VideoPlayer::shown_count`) when that
    /// window's current pass began: more since means it was in view.
    pub(super) mark: u64,
}

/// The windows a slot shows in.
pub(super) type Windows = HashMap<Option<ChatId>, Seen>;

/// A render pass of one window's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pass {
    pub(super) window: Option<ChatId>,
    pub(super) list: List,
    pub(super) number: u64,
    /// The list's width changed in its last [`RESIZE_PASSES`] passes. Rows
    /// move in and out of the overdraw from frame to frame then, and each
    /// return would open a new decoder, so no slot leaves on this pass.
    pub(super) resizing: bool,
}

/// History passes after a width change that count as still resizing.
const RESIZE_PASSES: u64 = 30;

/// As `pass` begins: the slot for `key` leaves the pass's window when its
/// row did not render in that window's previous pass of its list, and
/// stays while any window still shows it.
pub(super) fn survives(key: Key, windows: &mut Windows, pass: Pass) -> bool {
    if List::of(key) == pass.list && !pass.resizing {
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
    /// Every one, by what the windows last found, as a window is hidden
    /// or presented again.
    Recheck,
}

/// Record what a sweep found of a slot (`count`: its paint count now) and
/// return whether a window has it in view now; no window in `unseen` does.
/// A pass's sweep speaks for its window and the timer's for every one,
/// each by the paints since that window's mark; a recheck only reads.
pub(super) fn note_sweep(
    windows: &mut Windows,
    sweep: Sweep,
    count: u64,
    unseen: &HashSet<Option<ChatId>>,
) -> bool {
    for (window, seen) in windows.iter_mut() {
        if sweep == Sweep::Timer || sweep == Sweep::Pass(*window) {
            seen.in_view = count > seen.mark;
        }
    }
    windows
        .iter()
        .any(|(window, seen)| seen.in_view && !unseen.contains(window))
}

/// The windows whose clips count as out of view, of the `watched` ones:
/// those the platform does not present (`unpresented`), while another
/// window is presented. With none presented, none.
fn unseen<'a>(
    mut watched: impl Iterator<Item = &'a Option<ChatId>>,
    unpresented: &HashSet<Option<ChatId>>,
) -> HashSet<Option<ChatId>> {
    if watched.any(|window| !unpresented.contains(window)) {
        unpresented.clone()
    } else {
        HashSet::new()
    }
}

impl State {
    /// [`unseen`] now; a covered main window counts as not presented.
    pub(super) fn unseen(&self) -> HashSet<Option<ChatId>> {
        let mut out_of_sight = self.unpresented.clone();
        if self.main_covered {
            out_of_sight.insert(None);
        }
        unseen(self.watched.keys(), &out_of_sight)
    }

    /// The number of `window`'s current pass of `list`.
    pub(super) fn pass(&self, window: Option<ChatId>, list: List) -> u64 {
        self.passes.get(&(window, list)).copied().unwrap_or(0)
    }

    /// Note `window`'s history width at pass `number`; whether it is still
    /// resizing.
    pub(super) fn note_history_width(
        &mut self,
        window: Option<ChatId>,
        width: HistoryWidth,
        number: u64,
    ) -> bool {
        let entry = self.history_widths.entry(window).or_insert((width, 0));
        if entry.0 != width {
            *entry = (width, number);
        }
        entry.1 > 0 && number.saturating_sub(entry.1) <= RESIZE_PASSES
    }

    /// Begin `window`'s next pass of `list`: the slots whose rows did not
    /// render in its last one leave it. Returns the slots no window shows
    /// any more, to drop once the borrow is released.
    pub(super) fn begin_pass(
        &mut self,
        window: Option<ChatId>,
        list: List,
        resizing: bool,
    ) -> HashMap<Key, Slot> {
        let number = self.pass(window, list).saturating_add(1);
        self.passes.insert((window, list), number);
        let pass = Pass {
            window,
            list,
            number,
            resizing,
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
        let dropped = self.with(|state| state.begin_pass(window, List::Panel(panel), false));
        drop(dropped);
    }

    /// Called at the start of every frame of `window`, drawn in `host`,
    /// before anything in it renders: when its last frame drew no history
    /// pass where one was due (`history_drawn`; no chat open, Contacts,
    /// Calls…), nothing there can show a clip, so its history's clips
    /// leave it. The first frame starts watching whether the platform
    /// presents the window.
    pub(in crate::ui) fn frame_start(
        self: &Rc<Self>,
        window: Option<ChatId>,
        history_drawn: bool,
        host: &Window,
        cx: &mut App,
    ) {
        let Some((dropped, watched)) = self.with(|state| {
            let swept = state.swept.remove(&window);
            let dropped = if history_drawn && !swept {
                state.leave(window, Some(List::History))
            } else {
                HashMap::new()
            };
            (dropped, state.watched.contains_key(&window))
        }) else {
            return;
        };
        drop(dropped);
        if watched {
            return;
        }
        let videos = Rc::downgrade(self);
        let watch = host.observe_window_visibility(move |visibility, _, cx| {
            if let Some(videos) = videos.upgrade() {
                videos.rewatch(
                    |state| {
                        if visibility.is_visible() {
                            state.unpresented.remove(&window);
                        } else {
                            state.unpresented.insert(window);
                        }
                    },
                    cx,
                );
            }
        });
        let presented = host.is_visible();
        self.rewatch(
            |state| {
                state.watched.insert(window, watch);
                if !presented {
                    state.unpresented.insert(window);
                }
            },
            cx,
        );
    }

    /// Change which windows are watched or presented (`change`); when that
    /// changes which windows' clips count as out of view, recheck every
    /// clip by what each window last found: a window presented again has
    /// its clips that were in view there play again, a sleeping one with a
    /// new player, which redraws its history.
    fn rewatch(self: &Rc<Self>, change: impl FnOnce(&mut State), cx: &mut App) {
        let changed = self.with(|state| {
            let before = state.unseen();
            change(state);
            state.unseen() != before
        });
        if changed == Some(true) {
            let videos = self.clone();
            cx.defer(move |cx| videos.sweep(Sweep::Recheck, cx));
        }
    }

    /// The active chat window hides the main window whole, or no longer
    /// (`chat_window::cover`): the platform presents it, but it draws
    /// nothing, so its clips are out of sight as in a minimized window.
    pub(in crate::ui) fn set_main_covered(self: &Rc<Self>, covered: bool, cx: &mut App) {
        if self.with(|state| state.main_covered != covered) == Some(true) {
            self.rewatch(|state| state.main_covered = covered, cx);
        }
    }

    /// A viewer covers `window`'s chat: every clip leaves it, and so do
    /// the clicks to play asked for there that no player carries out yet.
    pub(in crate::ui) fn leave_window(&self, window: Option<ChatId>) {
        let dropped = self.with(|state| {
            let dropped = state.leave(window, None);
            let players = &state.players;
            state
                .on_demand
                .retain(|key, asked| asked.window != window || players.contains_key(key));
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
    pub(in crate::ui) fn forget_window(self: &Rc<Self>, chat: ChatId, cx: &mut App) {
        let window = Some(chat);
        let dropped = self.with(|state| {
            state.passes.retain(|(on, _), _| *on != window);
            state.views.remove(&window);
            state.swept.remove(&window);
            state.on_demand.retain(|_, asked| asked.window != window);
            state.leave(window, None)
        });
        drop(dropped);
        let mut gone = None;
        self.rewatch(
            |state| {
                gone = state.watched.remove(&window);
                state.unpresented.remove(&window);
            },
            cx,
        );
        drop(gone);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        List, Panel, Pass, RESIZE_PASSES, Seen, State, Sweep, Windows, note_sweep, survives, unseen,
    };
    use quill::bubble_layout::HistoryWidth;
    use quill::ids::ChatId;
    use std::collections::HashSet;

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
                        mark: 0,
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
            resizing: false,
        }
    }

    #[test]
    fn a_width_change_counts_as_resizing_for_a_stretch_of_passes() {
        let mut state = State::default();
        let (wide, narrow) = (HistoryWidth::new(500.), HistoryWidth::new(492.));
        assert!(!state.note_history_width(MAIN, wide, 1), "the first width");
        assert!(!state.note_history_width(MAIN, wide, 2));
        assert!(state.note_history_width(MAIN, narrow, 3));
        assert!(state.note_history_width(MAIN, narrow, 3 + RESIZE_PASSES));
        assert!(!state.note_history_width(MAIN, narrow, 4 + RESIZE_PASSES));
    }

    #[test]
    fn a_width_past_the_widest_layout_is_not_resizing() {
        let mut state = State::default();
        assert!(!state.note_history_width(MAIN, HistoryWidth::new(800.), 1));
        assert!(!state.note_history_width(MAIN, HistoryWidth::new(792.), 2));
    }

    #[test]
    fn a_resizing_pass_keeps_a_row_that_slipped_out_of_the_overdraw() {
        let row = (11, 204);
        let mut windows = shown(&[(MAIN, 3)]);
        let resizing = Pass {
            resizing: true,
            ..pass(MAIN, List::History, 40)
        };
        assert!(survives(row, &mut windows, resizing));
        assert!(!survives(row, &mut windows, pass(MAIN, List::History, 41)));
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
        let none = HashSet::new();
        let mut windows = shown(&[(MAIN, 1), (CHAT_WINDOW, 1)]);
        let mark = |windows: &mut Windows, window, count| {
            if let Some(seen) = windows.get_mut(&window) {
                seen.mark = count;
            }
        };
        // The chat window's pass began at 10 paints and painted it in
        // view; the main window's began at 11 and painted it only off
        // screen (overdraw).
        mark(&mut windows, CHAT_WINDOW, 10);
        mark(&mut windows, MAIN, 11);
        assert!(note_sweep(
            &mut windows,
            Sweep::Pass(CHAT_WINDOW),
            11,
            &none
        ));
        assert!(note_sweep(&mut windows, Sweep::Pass(MAIN), 11, &none));
        // Each window's pass is judged from its own mark: the main
        // window's starting does not undo the chat window's paints.
        mark(&mut windows, CHAT_WINDOW, 11);
        assert!(!note_sweep(
            &mut windows,
            Sweep::Pass(CHAT_WINDOW),
            11,
            &none
        ));
        // The timer judges for every window.
        assert!(note_sweep(&mut windows, Sweep::Timer, 12, &none));
        assert!(!note_sweep(&mut windows, Sweep::Timer, 11, &none));
    }

    #[test]
    fn a_clip_only_a_window_out_of_sight_shows_is_out_of_view() {
        let mut windows = shown(&[(MAIN, 1), (CHAT_WINDOW, 1)]);
        let hidden = HashSet::from([CHAT_WINDOW]);
        // Painted in view in the chat window, which the platform does not
        // present: out of view, until it is presented again (a recheck
        // reads what the windows last found).
        assert!(!note_sweep(
            &mut windows,
            Sweep::Pass(CHAT_WINDOW),
            1,
            &hidden
        ));
        assert!(!note_sweep(&mut windows, Sweep::Recheck, 1, &hidden));
        assert!(note_sweep(&mut windows, Sweep::Recheck, 1, &HashSet::new()));
        // In view in the main window too: in view.
        assert!(note_sweep(&mut windows, Sweep::Pass(MAIN), 1, &hidden));
    }

    #[test]
    fn only_a_window_out_of_sight_beside_one_presented_counts_unseen() {
        let both = [MAIN, CHAT_WINDOW];
        let hidden = |windows: &[Option<ChatId>]| -> HashSet<Option<ChatId>> {
            windows.iter().copied().collect()
        };
        assert_eq!(
            unseen(both.iter(), &hidden(&[CHAT_WINDOW])),
            hidden(&[CHAT_WINDOW])
        );
        assert_eq!(unseen(both.iter(), &hidden(&[MAIN])), hidden(&[MAIN]));
        // None presented (the app in the background), or a lone window
        // minimized: as before, nothing counts unseen.
        assert_eq!(unseen(both.iter(), &hidden(&both)), hidden(&[]));
        assert_eq!(unseen([MAIN].iter(), &hidden(&[MAIN])), hidden(&[]));
        assert_eq!(unseen(both.iter(), &hidden(&[])), hidden(&[]));
    }
}
