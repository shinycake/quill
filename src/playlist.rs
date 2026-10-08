//! Playlist ordering for the audio player bar.
//!
//! Pure logic, no UI: given the chat's playable messages in chronological
//! order, pick the track that follows or precedes the current one.
//! Telegram Desktop's `Media::Player::Instance::moveInPlaylist`
//! (`media/player/media_player_instance.cpp`) is the model: music honours
//! repeat (off / one / all) and order (in order / reverse / shuffle); voice
//! notes just continue to the next newer unread voice note.

/// Repeat mode of the music playlist (tdesktop `Media::RepeatMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepeatMode {
    #[default]
    Off,
    /// Repeat the current track.
    One,
    /// Wrap around at either end of the list.
    All,
}

impl RepeatMode {
    /// Next mode in the repeat button's cycle: off, one, all.
    pub fn cycled(self) -> Self {
        match self {
            Self::Off => Self::One,
            Self::One => Self::All,
            Self::All => Self::Off,
        }
    }
}

/// Order of the music playlist (tdesktop `Media::OrderMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OrderMode {
    /// Oldest message first, like the chat reads.
    #[default]
    InOrder,
    /// Newest message first.
    Reverse,
    Shuffle,
}

impl OrderMode {
    /// Next mode in the order button's cycle: in order, reverse, shuffle.
    pub fn cycled(self) -> Self {
        match self {
            Self::InOrder => Self::Reverse,
            Self::Reverse => Self::Shuffle,
            Self::Shuffle => Self::InOrder,
        }
    }
}

/// Which way to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    Previous,
}

/// Tracks already played in the current shuffle round, so Previous walks
/// back through them and Next never repeats one before the round is done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShuffleState {
    played: Vec<i64>,
    cursor: usize,
}

impl ShuffleState {
    pub fn clear(&mut self) {
        self.played.clear();
        self.cursor = 0;
    }

    /// Record `current` as the track now playing (a track picked by hand
    /// starts a fresh round).
    pub fn note_current(&mut self, current: i64) {
        if self.played.get(self.cursor) == Some(&current) {
            return;
        }
        self.played.clear();
        self.played.push(current);
        self.cursor = 0;
    }
}

/// How the playlist moves: the playing track, direction, and the modes.
#[derive(Clone, Copy, Debug)]
pub struct Move {
    pub current: i64,
    pub step: Step,
    /// True when the track ended by itself: repeat-one then replays it,
    /// while a manual Next/Previous skips on regardless.
    pub auto: bool,
    pub repeat: RepeatMode,
    pub order: OrderMode,
}

/// The track to play after `mv.current` in `ids` (message ids, oldest
/// first). `pick` returns a random index below its argument (injected so
/// tests are deterministic). Returns `None` at the end of the list.
pub fn next_track(
    ids: &[i64],
    mv: Move,
    shuffle: &mut ShuffleState,
    mut pick: impl FnMut(usize) -> usize,
) -> Option<i64> {
    if ids.is_empty() {
        return None;
    }
    if mv.auto && mv.repeat == RepeatMode::One {
        return Some(mv.current);
    }
    if mv.order == OrderMode::Shuffle {
        return shuffled(ids, mv, shuffle, &mut pick);
    }
    let Some(index) = ids.iter().position(|id| *id == mv.current) else {
        // The playing message left the window: start from an end.
        return match (mv.step, mv.order) {
            (Step::Next, OrderMode::Reverse) | (Step::Previous, _) => ids.last().copied(),
            _ => ids.first().copied(),
        };
    };
    let forward = (mv.step == Step::Next) == (mv.order == OrderMode::InOrder);
    let wrap = mv.repeat == RepeatMode::All;
    let len = ids.len();
    let target = if forward {
        if index + 1 < len {
            Some(index + 1)
        } else {
            wrap.then_some(0)
        }
    } else if index > 0 {
        Some(index - 1)
    } else {
        wrap.then_some(len - 1)
    };
    target.map(|i| ids[i])
}

fn shuffled(
    ids: &[i64],
    mv: Move,
    state: &mut ShuffleState,
    pick: &mut impl FnMut(usize) -> usize,
) -> Option<i64> {
    state.note_current(mv.current);
    if mv.step == Step::Previous {
        if state.cursor == 0 {
            return None;
        }
        state.cursor -= 1;
        return Some(state.played[state.cursor]);
    }
    if state.cursor + 1 < state.played.len() {
        state.cursor += 1;
        return Some(state.played[state.cursor]);
    }
    let mut fresh: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|id| !state.played.contains(id))
        .collect();
    if fresh.is_empty() {
        if mv.repeat != RepeatMode::All {
            return None;
        }
        // Round over: start another, not opening with the track that
        // just played.
        state.played.clear();
        state.played.push(mv.current);
        state.cursor = 0;
        fresh = ids.iter().copied().filter(|id| *id != mv.current).collect();
        if fresh.is_empty() {
            return Some(mv.current);
        }
    }
    let chosen = fresh[pick(fresh.len()).min(fresh.len() - 1)];
    state.played.push(chosen);
    state.cursor = state.played.len() - 1;
    Some(chosen)
}

/// One voice note in a chat, for [`next_voice`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceEntry {
    pub id: i64,
    pub listened: bool,
    pub outgoing: bool,
}

/// The voice note to continue with after `current` finishes: the next
/// newer one if it is still unread (`entries` oldest first). Outgoing and
/// already-listened notes end the chain, as in Telegram Desktop.
pub fn next_voice(entries: &[VoiceEntry], current: i64) -> Option<i64> {
    let after = entries.iter().position(|e| e.id == current)? + 1;
    entries
        .get(after)
        .filter(|e| !e.listened && !e.outgoing)
        .map(|e| e.id)
}

#[cfg(test)]
mod tests {
    use super::{
        Move, OrderMode, RepeatMode, ShuffleState, Step, VoiceEntry, next_track, next_voice,
    };

    fn mv(current: i64, step: Step, auto: bool, repeat: RepeatMode, order: OrderMode) -> Move {
        Move {
            current,
            step,
            auto,
            repeat,
            order,
        }
    }

    fn go(
        ids: &[i64],
        current: i64,
        step: Step,
        repeat: RepeatMode,
        order: OrderMode,
    ) -> Option<i64> {
        let m = mv(current, step, true, repeat, order);
        next_track(ids, m, &mut ShuffleState::default(), |_| 0)
    }

    #[test]
    fn in_order_walks_forward_and_stops_at_the_end() {
        let ids = [1, 2, 3];
        let g = |c, s| go(&ids, c, s, RepeatMode::Off, OrderMode::InOrder);
        assert_eq!(g(1, Step::Next), Some(2));
        assert_eq!(g(3, Step::Next), None);
        assert_eq!(g(1, Step::Previous), None);
        assert_eq!(g(3, Step::Previous), Some(2));
    }

    #[test]
    fn reverse_flips_both_directions() {
        let ids = [1, 2, 3];
        let g = |c, s| go(&ids, c, s, RepeatMode::Off, OrderMode::Reverse);
        assert_eq!(g(3, Step::Next), Some(2));
        assert_eq!(g(1, Step::Next), None);
        assert_eq!(g(1, Step::Previous), Some(2));
    }

    #[test]
    fn repeat_all_wraps_both_ways() {
        let ids = [1, 2, 3];
        let all = |c, s, o| go(&ids, c, s, RepeatMode::All, o);
        assert_eq!(all(3, Step::Next, OrderMode::InOrder), Some(1));
        assert_eq!(all(1, Step::Previous, OrderMode::InOrder), Some(3));
        assert_eq!(all(1, Step::Next, OrderMode::Reverse), Some(3));
    }

    #[test]
    fn repeat_one_replays_only_when_the_track_ends_by_itself() {
        let ids = [1, 2, 3];
        let one = |auto, step| {
            let m = mv(2, step, auto, RepeatMode::One, OrderMode::InOrder);
            next_track(&ids, m, &mut ShuffleState::default(), |_| 0)
        };
        assert_eq!(one(true, Step::Next), Some(2));
        assert_eq!(one(false, Step::Next), Some(3));
        assert_eq!(one(false, Step::Previous), Some(1));
    }

    #[test]
    fn a_track_that_left_the_window_restarts_from_an_end() {
        let ids = [1, 2, 3];
        let g = |s, o| go(&ids, 99, s, RepeatMode::Off, o);
        assert_eq!(g(Step::Next, OrderMode::InOrder), Some(1));
        assert_eq!(g(Step::Next, OrderMode::Reverse), Some(3));
        assert_eq!(
            go(&[], 1, Step::Next, RepeatMode::Off, OrderMode::InOrder),
            None
        );
    }

    #[test]
    fn shuffle_plays_every_track_once_then_stops() {
        let ids = [1, 2, 3, 4];
        let mut state = ShuffleState::default();
        let mut cur = 2;
        let mut seen = vec![cur];
        let m = |c| mv(c, Step::Next, true, RepeatMode::Off, OrderMode::Shuffle);
        while let Some(n) = next_track(&ids, m(cur), &mut state, |n| n - 1) {
            seen.push(n);
            cur = n;
        }
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2, 3, 4]);
    }

    #[test]
    fn shuffle_previous_retraces_and_next_replays_the_same_path() {
        let ids = [1, 2, 3, 4];
        let mut state = ShuffleState::default();
        let mut g = |cur, step| {
            let m = mv(cur, step, false, RepeatMode::Off, OrderMode::Shuffle);
            next_track(&ids, m, &mut state, |_| 0)
        };
        let b = g(1, Step::Next).unwrap();
        let c = g(b, Step::Next).unwrap();
        assert_eq!(g(c, Step::Previous), Some(b));
        assert_eq!(g(b, Step::Previous), Some(1));
        assert_eq!(g(1, Step::Previous), None);
        assert_eq!(g(1, Step::Next), Some(b));
        assert_eq!(g(b, Step::Next), Some(c));
    }

    #[test]
    fn shuffle_with_repeat_all_starts_a_new_round() {
        let ids = [1, 2];
        let mut state = ShuffleState::default();
        let mut g = |cur| {
            let m = mv(cur, Step::Next, true, RepeatMode::All, OrderMode::Shuffle);
            next_track(&ids, m, &mut state, |_| 0)
        };
        assert_eq!(g(1), Some(2));
        assert_eq!(g(2), Some(1));
        assert_eq!(g(1), Some(2));
    }

    #[test]
    fn voice_chain_continues_only_through_unread_incoming_notes() {
        let e = |id, listened, outgoing| VoiceEntry {
            id,
            listened,
            outgoing,
        };
        let list = [
            e(1, true, false),
            e(2, false, false),
            e(3, false, false),
            e(4, true, false),
            e(5, false, true),
        ];
        assert_eq!(next_voice(&list, 1), Some(2));
        assert_eq!(next_voice(&list, 2), Some(3));
        assert_eq!(next_voice(&list, 3), None);
        assert_eq!(next_voice(&list, 4), None);
        assert_eq!(next_voice(&list, 9), None);
    }

    #[test]
    fn mode_buttons_cycle() {
        assert_eq!(RepeatMode::Off.cycled().cycled().cycled(), RepeatMode::Off);
        assert_eq!(OrderMode::InOrder.cycled(), OrderMode::Reverse);
        assert_eq!(OrderMode::Shuffle.cycled(), OrderMode::InOrder);
    }
}
