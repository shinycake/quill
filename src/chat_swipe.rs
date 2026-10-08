//! Chat-list row swipe actions (tdesktop "Chat list quick action"):
//! a horizontal two-finger trackpad swipe slides a row to the left,
//! revealing the configured action; past the threshold the action runs
//! when the finger lifts, otherwise the row springs back.
//!
//! Pure core, no GPUI. The state machine mirrors
//! `ui/controls/swipe_handler.cpp` (`SetupSwipeHandler`, wheel branch) and
//! the row glue in `dialogs/dialogs_widget.cpp` (`setupSwipeBack`) and
//! `dialogs/dialogs_inner_widget.cpp` (`setSwipeContextData`). Time is
//! passed in as milliseconds so every transition is deterministic.

use serde::{Deserialize, Serialize};

/// `kThresholdWidth` (swipe_handler.cpp:77): travel for ratio 1.
pub const THRESHOLD: f32 = 50.0;
/// `kMaxRatio` (swipe_handler.cpp:78): cap applied when the finger lifts.
pub const MAX_RATIO: f32 = 1.5;
/// `kSwipeSlow` (swipe_handler.cpp:23): scroll deltas are scaled by this.
pub const WHEEL_SLOW: f32 = 0.2;
/// `st::slideWrapDuration` (lib_ui basic.style:100): the reach bounce,
/// the spring back, and the delay before the action runs.
pub const SLIDE_MS: u64 = 150;
/// `kOrientationThreshold` (swipe_handler.cpp:`updateWith`): |dx| - |dy|
/// margin (in slowed units) that decides horizontal vs vertical.
const ORIENTATION_MARGIN: f32 = 1.0;
/// `kResetReachedOn`: the reach flag re-arms below this ratio.
const RESET_REACHED_ON: f32 = 0.95;
/// A gesture that stops sending events (no `Ended`) is abandoned after
/// this long; platforms always end a phased scroll, so this is a safety
/// net for a lost event, not a normal path.
pub const STALE_GESTURE_MS: u64 = 600;
/// `st::dialogsQuickActionSize` (dialogs.style:835): icon box.
pub const ICON_SIZE: f32 = 20.0;
/// Width of the icon+label block pinned to the row's right edge
/// (`dialogs_layout.cpp:1048`, `quickWidth = size * 3`).
pub const ACTION_WIDTH: f32 = ICON_SIZE * 3.0;

// ---------------------------------------------------------------------
// Settings value and labels
// ---------------------------------------------------------------------

/// `Dialogs::Ui::QuickDialogAction` (dialogs_quick_action.h). tdesktop's
/// default is `Disabled` (`core_settings.h:1222`), where a swipe changes
/// the folder instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwipeAction {
    #[default]
    Disabled,
    Mute,
    Pin,
    Read,
    Archive,
    Delete,
}

impl SwipeAction {
    /// Settings order (`settings_chat.cpp` radio list).
    pub const ALL: [SwipeAction; 6] = [
        SwipeAction::Mute,
        SwipeAction::Pin,
        SwipeAction::Read,
        SwipeAction::Archive,
        SwipeAction::Delete,
        SwipeAction::Disabled,
    ];

    /// Settings radio label (`lng_settings_quick_dialog_action_*`; the
    /// "disabled" string reads "Change folder" there).
    pub fn settings_label(self) -> &'static str {
        match self {
            SwipeAction::Mute => "Mute",
            SwipeAction::Pin => "Pin",
            SwipeAction::Read => "Read",
            SwipeAction::Archive => "Archive",
            SwipeAction::Delete => "Delete",
            SwipeAction::Disabled => "Disabled",
        }
    }
}

/// What the row would do right now, with the toggle resolved
/// (`QuickDialogActionLabel`, dialogs_quick_action_context.h:25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeLabel {
    Mute,
    Unmute,
    Pin,
    Unpin,
    Read,
    Unread,
    Archive,
    Unarchive,
    Delete,
    Disabled,
}

impl SwipeLabel {
    /// `lng_settings_quick_dialog_action_*` strings.
    pub fn text(self) -> &'static str {
        match self {
            SwipeLabel::Mute => "Mute",
            SwipeLabel::Unmute => "Unmute",
            SwipeLabel::Pin => "Pin",
            SwipeLabel::Unpin => "Unpin",
            SwipeLabel::Read => "Read",
            SwipeLabel::Unread => "Unread",
            SwipeLabel::Archive => "Archive",
            SwipeLabel::Unarchive => "Unarchive",
            SwipeLabel::Delete => "Delete",
            SwipeLabel::Disabled => "Disabled",
        }
    }

    /// `lng_quick_dialog_action_toast_*_success` (shown after the action).
    pub fn toast(self) -> &'static str {
        match self {
            SwipeLabel::Mute => "Notifications for this chat have been muted.",
            SwipeLabel::Unmute => "Notifications enabled for this chat.",
            SwipeLabel::Pin => "The chat has been pinned.",
            SwipeLabel::Unpin => "The chat has been unpinned.",
            SwipeLabel::Read => "The chat has been marked as read.",
            SwipeLabel::Unread => "The chat has been marked as unread.",
            SwipeLabel::Archive => "The chat has been archived.",
            SwipeLabel::Unarchive => "The chat has been unarchived.",
            SwipeLabel::Delete | SwipeLabel::Disabled => "",
        }
    }

    /// Label font size: 13 px, shrunk to fit the 60 px block
    /// (`SwipeActionFont`, dialogs_quick_action.cpp:34, min 5; Quill keeps
    /// 8 px as the floor for legibility).
    pub fn font_px(self) -> f32 {
        // Average semibold glyph advance is about 0.56 em.
        let chars = self.text().chars().count().max(1) as f32;
        (ACTION_WIDTH / (chars * 0.56)).clamp(8.0, 13.0).floor()
    }
}

/// The chat facts the label depends on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChatSwipeFacts {
    pub muted: bool,
    /// Pinned in the list the chat sits in (main or archive).
    pub pinned: bool,
    /// Unread messages or the manual unread mark.
    pub unread: bool,
    pub archived: bool,
    /// The chat with yourself: it cannot be muted.
    pub is_saved: bool,
    /// Archive/unarchive is offered for this chat.
    pub can_archive: bool,
    /// The chat can be removed from the list (the Delete confirm exists).
    pub can_delete: bool,
}

/// `ResolveQuickDialogLabel(history, action, filterId)`
/// (dialogs_quick_action.cpp:`ResolveQuickDialogLabel`).
pub fn resolve_label(action: SwipeAction, facts: ChatSwipeFacts) -> SwipeLabel {
    match action {
        SwipeAction::Disabled => SwipeLabel::Disabled,
        SwipeAction::Mute if facts.is_saved => SwipeLabel::Disabled,
        SwipeAction::Mute if facts.muted => SwipeLabel::Unmute,
        SwipeAction::Mute => SwipeLabel::Mute,
        SwipeAction::Pin if facts.pinned => SwipeLabel::Unpin,
        SwipeAction::Pin => SwipeLabel::Pin,
        SwipeAction::Read if facts.unread => SwipeLabel::Read,
        SwipeAction::Read => SwipeLabel::Unread,
        SwipeAction::Archive if !facts.can_archive => SwipeLabel::Disabled,
        SwipeAction::Archive if facts.archived => SwipeLabel::Unarchive,
        SwipeAction::Archive => SwipeLabel::Archive,
        SwipeAction::Delete if !facts.can_delete => SwipeLabel::Disabled,
        SwipeAction::Delete => SwipeLabel::Delete,
    }
}

// ---------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------

/// `DampedOverswipe` (swipe_handler.cpp:36): logarithmic damping of the
/// travel past the threshold (scale 1).
fn damped_overswipe(travel: f32) -> f32 {
    16.0 * (1.0 + travel.max(0.0) / 10.0).ln()
}

/// `exactTranslation` for a ratio (`updateRatio`, swipe_handler.cpp:127):
/// linear up to the threshold, damped beyond it.
pub fn exact_translation(ratio: f32) -> f32 {
    let ratio = ratio.max(0.0);
    ratio.min(1.0) * THRESHOLD + damped_overswipe((ratio - 1.0).max(0.0) * THRESHOLD)
}

/// How far the row's content slides left, in px: tdesktop paints the row
/// at twice the exact translation (`dialogs_layout.cpp:480`, `* -2`).
pub fn row_shift(ratio: f32) -> f32 {
    exact_translation(ratio) * 2.0
}

/// Pure presentation of one swiped row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowSwipe {
    pub ratio: f32,
    /// 0..1 bounce progress once the threshold was reached
    /// (`SwipeContextData::reachRatio`).
    pub reach: f32,
    /// Content shift to the left, px; also the revealed panel width.
    pub shift: f32,
}

impl RowSwipe {
    /// Radius of the reach flood circle (`dialogs_layout.cpp:1036`).
    pub fn reach_radius(&self) -> f32 {
        self.shift * self.reach
    }
}

// ---------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------

/// Scroll phase of one wheel event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Orientation {
    Undecided,
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct Anim {
    from: f32,
    to: f32,
    start_ms: u64,
    duration_ms: u64,
}

impl Anim {
    fn value(&self, now: u64) -> f32 {
        if self.duration_ms == 0 || now >= self.start_ms + self.duration_ms {
            return self.to;
        }
        let t = (now.saturating_sub(self.start_ms)) as f32 / self.duration_ms as f32;
        self.from + (self.to - self.from) * t
    }

    fn running(&self, now: u64) -> bool {
        self.duration_ms > 0 && now < self.start_ms + self.duration_ms
    }
}

#[derive(Debug)]
struct Gesture<K> {
    key: Option<K>,
    /// The first nonzero horizontal delta fixed the direction.
    direction_set: bool,
    /// +1 when fingers moved left (the only direction that swipes a row).
    direction: f32,
    orientation: Orientation,
    /// Accumulated slowed delta (`state->delta`), x then y.
    delta: (f32, f32),
    ratio: f32,
    reached: bool,
    reach: Anim,
    last_event_ms: u64,
}

#[derive(Debug)]
struct Release<K> {
    key: K,
    ratio: Anim,
    /// Reach value frozen at release (`reachRatio` stays until cleared).
    reach: f32,
    /// Run the action when the animation ends.
    fire: bool,
}

/// What a wheel event asks of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Feed {
    /// The gesture is a horizontal swipe on a row: do not let the list
    /// scroll (tdesktop `scroll->disableScroll(true)`).
    pub consumed: bool,
    /// Something changed that needs a redraw.
    pub redraw: bool,
}

/// One swipe gesture at a time plus rows still springing back.
#[derive(Debug)]
pub struct SwipeMachine<K> {
    gesture: Option<Gesture<K>>,
    releases: Vec<Release<K>>,
}

impl<K> Default for SwipeMachine<K> {
    fn default() -> Self {
        Self {
            gesture: None,
            releases: Vec::new(),
        }
    }
}

impl<K: Copy + PartialEq> SwipeMachine<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one precise-scroll event. `key` is the row under the pointer
    /// that can run the action, or `None` when there is none (no row,
    /// action disabled for it): such a gesture is locked out like a
    /// vertical one. `dx`/`dy` are the raw scroll deltas in px.
    pub fn feed(&mut self, phase: Phase, dx: f32, dy: f32, key: Option<K>, now: u64) -> Feed {
        match phase {
            Phase::Ended => {
                let redraw = self.end(now, true);
                Feed {
                    consumed: false,
                    redraw,
                }
            }
            Phase::Cancelled => {
                let redraw = self.end(now, false);
                Feed {
                    consumed: false,
                    redraw,
                }
            }
            Phase::Started => {
                // A new scroll begins: reset any lost end (`ScrollBegin`).
                self.end(now, true);
                self.gesture = Some(Gesture {
                    key: None,
                    direction_set: false,
                    direction: 1.0,
                    orientation: Orientation::Undecided,
                    delta: (0.0, 0.0),
                    ratio: 0.0,
                    reached: false,
                    reach: Anim {
                        from: 0.0,
                        to: 0.0,
                        start_ms: now,
                        duration_ms: 0,
                    },
                    last_event_ms: now,
                });
                self.update(dx, dy, key, now, true)
            }
            Phase::Moved => {
                // Without a started gesture this is a plain wheel or
                // momentum tail: not ours.
                if self.gesture.is_none() {
                    return Feed::default();
                }
                self.update(dx, dy, key, now, false)
            }
        }
    }

    fn update(&mut self, dx: f32, dy: f32, key: Option<K>, now: u64, first: bool) -> Feed {
        let Some(g) = self.gesture.as_mut() else {
            return Feed::default();
        };
        g.last_event_ms = now;
        // `args.delta = state->delta - ScrollDeltaF(w) * kSwipeSlow`.
        let args = (g.delta.0 - dx * WHEEL_SLOW, g.delta.1 - dy * WHEEL_SLOW);
        if first || !g.direction_set {
            // `fillFinishByTop`: only a nonzero horizontal delta fixes the
            // direction; the event's own delta is dropped (`delta = {}`).
            if args.0 != 0.0 {
                g.direction_set = true;
                g.direction = if args.0 < 0.0 { -1.0 } else { 1.0 };
                g.key = key;
                // Wrong direction (swiping right) or no row action:
                // leave the event to the list.
                if g.key.is_none() || g.direction < 0.0 {
                    g.orientation = Orientation::Vertical;
                }
            }
            return Feed::default();
        }
        match g.orientation {
            Orientation::Vertical => Feed::default(),
            Orientation::Undecided => {
                g.delta = args;
                let diff = args.0.abs() - args.1.abs();
                if diff > ORIENTATION_MARGIN {
                    g.orientation = Orientation::Horizontal;
                } else if diff < -ORIENTATION_MARGIN {
                    g.orientation = Orientation::Vertical;
                }
                Feed::default()
            }
            Orientation::Horizontal => {
                g.delta = args;
                let ratio = (args.0 * g.direction / THRESHOLD).max(0.0);
                g.ratio = ratio;
                if !g.reached && ratio >= 1.0 {
                    g.reached = true;
                    g.reach = Anim {
                        from: 0.0,
                        to: 1.0,
                        start_ms: now,
                        duration_ms: SLIDE_MS,
                    };
                } else if g.reached && ratio < RESET_REACHED_ON {
                    let current = g.reach.value(now);
                    g.reach = Anim {
                        from: current,
                        to: 0.0,
                        start_ms: now,
                        duration_ms: SLIDE_MS,
                    };
                    g.reached = false;
                }
                Feed {
                    consumed: true,
                    redraw: true,
                }
            }
        }
    }

    /// `processEnd`: a horizontal swipe springs back over
    /// `min(1, ratio) * slideWrapDuration`, running the action after
    /// `slideWrapDuration` when the ratio reached 1 (`fire`).
    fn end(&mut self, now: u64, fire: bool) -> bool {
        let Some(g) = self.gesture.take() else {
            return false;
        };
        if g.orientation != Orientation::Horizontal {
            return false;
        }
        let Some(key) = g.key else {
            return false;
        };
        let raw = (g.delta.0 * g.direction / THRESHOLD).max(0.0);
        let ratio = raw.clamp(0.0, MAX_RATIO);
        let reach = g.reach.value(now);
        self.releases.push(Release {
            key,
            ratio: Anim {
                from: ratio,
                to: 0.0,
                start_ms: now,
                duration_ms: (ratio.min(1.0) * SLIDE_MS as f32).round() as u64,
            },
            reach,
            fire: fire && ratio >= 1.0,
        });
        true
    }

    /// Abandon a gesture that went quiet without an end event, and drop
    /// finished springs. Returns the key whose action is due, if any.
    pub fn poll(&mut self, now: u64) -> Option<K> {
        if self
            .gesture
            .as_ref()
            .is_some_and(|g| now.saturating_sub(g.last_event_ms) > STALE_GESTURE_MS)
        {
            self.end(now, false);
        }
        let mut due = None;
        self.releases.retain(|r| {
            if r.ratio.running(now) {
                return true;
            }
            if r.fire && due.is_none() {
                due = Some(r.key);
            }
            false
        });
        due
    }

    /// Presentation of `key`'s row, `None` when it is not swiped.
    pub fn row(&self, key: K, now: u64) -> Option<RowSwipe> {
        if let Some(g) = &self.gesture
            && g.orientation == Orientation::Horizontal
            && g.key == Some(key)
            && g.ratio > 0.0
        {
            return Some(RowSwipe {
                ratio: g.ratio,
                reach: g.reach.value(now),
                shift: row_shift(g.ratio),
            });
        }
        self.releases
            .iter()
            .find(|r| r.key == key)
            .map(|r| {
                let ratio = r.ratio.value(now);
                RowSwipe {
                    ratio,
                    reach: r.reach,
                    shift: row_shift(ratio),
                }
            })
            .filter(|row| row.shift > 0.0)
    }

    /// Frames are needed (reach bounce, spring back, pending action).
    /// A held horizontal swipe redraws from its events instead.
    pub fn animating(&self, now: u64) -> bool {
        !self.releases.is_empty() || self.gesture.as_ref().is_some_and(|g| g.reach.running(now))
    }

    /// A gesture is in progress on a row (the list must not scroll).
    pub fn swiping(&self) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|g| g.orientation == Orientation::Horizontal)
    }

    /// Drop everything (chat list changed under the gesture).
    pub fn reset(&mut self) {
        self.gesture = None;
        self.releases.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChatSwipeFacts, MAX_RATIO, Phase, SLIDE_MS, STALE_GESTURE_MS, SwipeAction, SwipeLabel,
        SwipeMachine, THRESHOLD, WHEEL_SLOW, exact_translation, resolve_label, row_shift,
    };

    /// Raw scroll px that move the ratio by one threshold.
    const FULL: f32 = THRESHOLD / WHEEL_SLOW;

    fn start(m: &mut SwipeMachine<u32>, key: Option<u32>) {
        // Mac sends a MayBegin/Began event with no travel first.
        m.feed(Phase::Started, 0.0, 0.0, key, 0);
        // Fingers moving left: negative scroll x. The first nonzero x
        // fixes the direction (its delta is dropped); then the
        // orientation is decided.
        m.feed(Phase::Moved, -1.0, 0.0, key, 8);
    }

    #[test]
    fn horizontal_lock_consumes_and_tracks_ratio() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(7));
        // Orientation decided on this event; ratio starts at the next.
        let f = m.feed(Phase::Moved, -20.0, 1.0, Some(7), 16);
        assert!(!f.consumed);
        let f = m.feed(Phase::Moved, -FULL * 0.5, 0.0, Some(7), 24);
        assert!(f.consumed && f.redraw);
        let row = m.row(7, 24).unwrap();
        // Slowed accumulation: 20 + FULL/2 raw px.
        let expected = (20.0 + FULL * 0.5) * WHEEL_SLOW / THRESHOLD;
        assert!((row.ratio - expected).abs() < 1e-4);
        assert!(m.swiping());
        assert!(m.row(8, 24).is_none());
    }

    #[test]
    fn vertical_scroll_is_never_hijacked() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(1));
        // Vertical dominates: locked out for the rest of the gesture,
        // even if the finger later drifts sideways.
        let f = m.feed(Phase::Moved, -2.0, 60.0, Some(1), 16);
        assert!(!f.consumed);
        for t in 1..20 {
            let f = m.feed(Phase::Moved, -100.0, 0.0, Some(1), 16 + t * 8);
            assert!(!f.consumed, "locked vertical gesture stayed vertical");
        }
        assert!(m.row(1, 200).is_none());
        assert!(!m.swiping());
        m.feed(Phase::Ended, 0.0, 0.0, Some(1), 300);
        assert!(!m.animating(300));
    }

    #[test]
    fn rightward_swipe_and_missing_row_are_locked_out() {
        let mut m = SwipeMachine::new();
        m.feed(Phase::Started, 0.0, 0.0, Some(1), 0);
        m.feed(Phase::Moved, 5.0, 0.0, Some(1), 8);
        let f = m.feed(Phase::Moved, 400.0, 0.0, Some(1), 16);
        assert!(!f.consumed);
        assert!(m.row(1, 16).is_none());

        let mut m = SwipeMachine::new();
        start(&mut m, None);
        let f = m.feed(Phase::Moved, -400.0, 0.0, None, 16);
        assert!(!f.consumed);
    }

    #[test]
    fn momentum_without_a_start_is_ignored() {
        let mut m: SwipeMachine<u32> = SwipeMachine::new();
        let f = m.feed(Phase::Moved, -500.0, 0.0, Some(1), 0);
        assert_eq!(f, Default::default());
        assert!(m.row(1, 0).is_none());
        // Non-phased platforms only ever send Moved: nothing happens.
        for t in 0..10 {
            m.feed(Phase::Moved, -500.0, 0.0, Some(1), t);
        }
        assert!(!m.swiping() && !m.animating(10));
    }

    #[test]
    fn release_below_threshold_springs_back_without_firing() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(3));
        m.feed(Phase::Moved, -10.0, 0.0, Some(3), 16);
        m.feed(Phase::Moved, -FULL * 0.6, 0.0, Some(3), 24);
        let held = m.row(3, 24).unwrap().ratio;
        assert!(held > 0.5 && held < 1.0);
        m.feed(Phase::Ended, 0.0, 0.0, Some(3), 30);
        assert!(m.animating(30));
        // Duration is ratio * 150 ms, linear back to 0.
        let half = 30 + (held * SLIDE_MS as f32 / 2.0) as u64;
        let mid = m.row(3, half).unwrap().ratio;
        assert!((mid - held / 2.0).abs() < 0.05, "mid={mid} held={held}");
        assert_eq!(m.poll(30 + SLIDE_MS), None);
        assert!(m.row(3, 30 + SLIDE_MS).is_none());
        assert!(!m.animating(30 + SLIDE_MS));
    }

    #[test]
    fn release_past_threshold_fires_after_the_slide_duration() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(9));
        m.feed(Phase::Moved, -10.0, 0.0, Some(9), 16);
        m.feed(Phase::Moved, -FULL * 1.1, 0.0, Some(9), 24);
        // Reached: the bounce animation runs.
        let reach_mid = m.row(9, 24 + SLIDE_MS / 2).unwrap().reach;
        assert!((reach_mid - 0.5).abs() < 0.05);
        assert!((m.row(9, 24 + SLIDE_MS).unwrap().reach - 1.0).abs() < 1e-4);
        m.feed(Phase::Ended, 0.0, 0.0, Some(9), 400);
        // Full-duration spring (ratio >= 1) and the action is due at its
        // end, exactly once.
        assert_eq!(m.poll(400 + SLIDE_MS - 1), None);
        assert_eq!(m.poll(400 + SLIDE_MS), Some(9));
        assert_eq!(m.poll(400 + SLIDE_MS + 1), None);
    }

    #[test]
    fn dragging_back_below_the_reach_mark_rearms_and_cancels_the_fire() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(2));
        m.feed(Phase::Moved, -10.0, 0.0, Some(2), 16);
        m.feed(Phase::Moved, -FULL * 1.2, 0.0, Some(2), 24);
        m.feed(Phase::Moved, FULL * 0.9, 0.0, Some(2), 300);
        let row = m.row(2, 300).unwrap();
        assert!(row.ratio < 0.95);
        m.feed(Phase::Ended, 0.0, 0.0, Some(2), 320);
        assert_eq!(m.poll(320 + SLIDE_MS), None, "ended below the threshold");
    }

    #[test]
    fn cancelled_gesture_never_fires() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(4));
        m.feed(Phase::Moved, -10.0, 0.0, Some(4), 16);
        m.feed(Phase::Moved, -FULL * 1.4, 0.0, Some(4), 24);
        m.feed(Phase::Cancelled, 0.0, 0.0, Some(4), 40);
        assert_eq!(m.poll(40 + SLIDE_MS), None);
        assert!(!m.animating(40 + SLIDE_MS));
    }

    #[test]
    fn ratio_caps_at_max_on_release() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(5));
        m.feed(Phase::Moved, -10.0, 0.0, Some(5), 16);
        m.feed(Phase::Moved, -FULL * 10.0, 0.0, Some(5), 24);
        m.feed(Phase::Ended, 0.0, 0.0, Some(5), 30);
        let start_ratio = m.row(5, 30).unwrap().ratio;
        assert!((start_ratio - MAX_RATIO).abs() < 1e-4);
    }

    #[test]
    fn stale_gesture_is_abandoned() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(6));
        m.feed(Phase::Moved, -10.0, 0.0, Some(6), 16);
        m.feed(Phase::Moved, -FULL * 1.3, 0.0, Some(6), 24);
        assert_eq!(m.poll(24 + STALE_GESTURE_MS), None);
        assert!(m.swiping());
        m.poll(24 + STALE_GESTURE_MS + 1);
        assert!(!m.swiping());
        // Abandoned means springs back without running the action.
        assert_eq!(m.poll(24 + STALE_GESTURE_MS + 1 + SLIDE_MS), None);
    }

    #[test]
    fn started_resets_a_lost_end() {
        let mut m = SwipeMachine::new();
        start(&mut m, Some(1));
        m.feed(Phase::Moved, -10.0, 0.0, Some(1), 16);
        m.feed(Phase::Moved, -FULL * 1.3, 0.0, Some(1), 24);
        // No Ended; a new scroll starts on another row.
        m.feed(Phase::Started, 0.0, 0.0, Some(2), 100);
        // The old row springs back and still counts as released past the
        // threshold, so its action runs once.
        assert_eq!(m.poll(100 + SLIDE_MS), Some(1));
    }

    #[test]
    fn translation_matches_tdesktop_curve() {
        assert_eq!(exact_translation(0.0), 0.0);
        assert!((exact_translation(0.5) - 25.0).abs() < 1e-4);
        assert!((exact_translation(1.0) - 50.0).abs() < 1e-4);
        // Past the threshold only the damped log grows: 16 ln(1 + 25/10).
        let over = exact_translation(1.5) - 50.0;
        assert!((over - 16.0 * 3.5_f32.ln()).abs() < 1e-3);
        // Rows paint at twice the exact translation.
        assert!((row_shift(1.0) - 100.0).abs() < 1e-4);
        assert!(row_shift(1.5) < 2.0 * (50.0 + 25.0));
        assert_eq!(row_shift(-1.0), 0.0);
    }

    #[test]
    fn labels_resolve_the_toggle() {
        let base = ChatSwipeFacts {
            can_archive: true,
            can_delete: true,
            ..Default::default()
        };
        let label = |a, f| resolve_label(a, f);
        assert_eq!(label(SwipeAction::Mute, base), SwipeLabel::Mute);
        assert_eq!(
            label(
                SwipeAction::Mute,
                ChatSwipeFacts {
                    muted: true,
                    ..base
                }
            ),
            SwipeLabel::Unmute
        );
        assert_eq!(
            label(
                SwipeAction::Mute,
                ChatSwipeFacts {
                    is_saved: true,
                    ..base
                }
            ),
            SwipeLabel::Disabled
        );
        assert_eq!(label(SwipeAction::Pin, base), SwipeLabel::Pin);
        assert_eq!(
            label(
                SwipeAction::Pin,
                ChatSwipeFacts {
                    pinned: true,
                    ..base
                }
            ),
            SwipeLabel::Unpin
        );
        assert_eq!(label(SwipeAction::Read, base), SwipeLabel::Unread);
        assert_eq!(
            label(
                SwipeAction::Read,
                ChatSwipeFacts {
                    unread: true,
                    ..base
                }
            ),
            SwipeLabel::Read
        );
        assert_eq!(label(SwipeAction::Archive, base), SwipeLabel::Archive);
        assert_eq!(
            label(
                SwipeAction::Archive,
                ChatSwipeFacts {
                    archived: true,
                    ..base
                }
            ),
            SwipeLabel::Unarchive
        );
        assert_eq!(
            label(
                SwipeAction::Archive,
                ChatSwipeFacts {
                    can_archive: false,
                    ..base
                }
            ),
            SwipeLabel::Disabled
        );
        assert_eq!(label(SwipeAction::Delete, base), SwipeLabel::Delete);
        assert_eq!(label(SwipeAction::Disabled, base), SwipeLabel::Disabled);
        assert_eq!(SwipeAction::default(), SwipeAction::Disabled);
    }

    #[test]
    fn label_fonts_shrink_to_fit() {
        assert_eq!(SwipeLabel::Pin.font_px(), 13.0);
        assert!(SwipeLabel::Unarchive.font_px() < 13.0);
        assert!(SwipeLabel::Unarchive.font_px() >= 8.0);
    }

    #[test]
    fn settings_value_roundtrips_lowercase() {
        for action in SwipeAction::ALL {
            let json = serde_json::to_string(&action).unwrap();
            assert_eq!(json, format!("\"{}\"", json.trim_matches('"')));
            assert_eq!(serde_json::from_str::<SwipeAction>(&json).unwrap(), action);
        }
        assert_eq!(
            serde_json::to_string(&SwipeAction::Mute).unwrap(),
            "\"mute\""
        );
    }
}
