//! A surface that comes and goes with a piece of state: toasts, the bars
//! above the composer, the pinned bar, the emoji panel, the corner
//! buttons. The state is the truth (`Option<T>`, drawn when `Some`);
//! [`Presence`] only follows it, gliding `shown` from 0 to 1 when the
//! state appears and back when it goes, and keeps handing out the last
//! `T` while the surface leaves, so it can be drawn on its way out.
//!
//! Telegram Desktop's equivalents: `Ui::SlideWrap` (`slideWrapDuration`,
//! 150 ms, `lib_ui/ui/basic.style:100`) for bars that open and close,
//! `Ui::Toast::Instance` (`durationFadeIn` 200 ms,
//! `lib_ui/ui/widgets/widgets.style:1556`), `TabbedPanel::showAnimated`
//! (`emojiPanShowDuration` 200 ms, `chat_helpers.style:518`) and
//! `CornerButtons::updateVisibility` (`historyToDownDuration` 150 ms,
//! `chat_helpers.style:1227`). Exits run in `universalDuration` (120 ms,
//! `basic.style:135`): faster than entrances, so a dismissed surface is
//! out of the way at once.
//!
//! Timing lives here and in [`super::motion::Glide`]; the views read the
//! values while rendering and ask the frame clock for frames only while
//! [`Presence::moving`]. Tests step a presence on an explicit clock.
#![deny(clippy::indexing_slicing)]

use super::motion::{Glide, ease_out_circ, elapsed_since, progress};
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// `st::boxDuration` / `emojiPanShowDuration` / toast `durationFadeIn`.
pub(crate) const ENTER: Duration = Duration::from_millis(200);
/// `st::slideWrapDuration`: a bar sliding open.
pub(crate) const SLIDE: Duration = Duration::from_millis(150);
/// `st::historyToDownDuration`: a corner button fading in.
pub(crate) const CORNER: Duration = Duration::from_millis(150);
/// `st::universalDuration`: every exit.
pub(crate) const EXIT: Duration = Duration::from_millis(120);
/// `st::defaultMessageBar.duration` (`chat_helpers.style:1041`): the
/// pinned bar's text swapping to another message.
pub(crate) const SWAP: Duration = Duration::from_millis(160);

/// What a surface draws this frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Drawn<T> {
    /// The state to draw: the live one, or the last one while leaving.
    pub(crate) value: T,
    /// How far the surface is in, `0..=1`.
    pub(crate) shown: f32,
    /// The state is gone; `value` is on its way out.
    pub(crate) leaving: bool,
    /// How far the content has swapped from `previous` to `value`,
    /// `0..=1` (1: no swap under way).
    pub(crate) swap: f32,
    /// The content shown before `value`, while a swap is under way.
    pub(crate) previous: Option<T>,
}

impl<T> Drawn<T> {
    /// A settled, fully shown surface.
    pub(crate) fn settled(value: T) -> Self {
        Self {
            value,
            shown: 1.,
            leaving: false,
            swap: 1.,
            previous: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Presence<T> {
    shown: Glide,
    last: Option<T>,
    previous: Option<T>,
    /// When `last` changed while the surface was shown.
    changed: Option<Instant>,
    /// Whose surface this is (a chat id): a change snaps, as the surface
    /// was never shown there.
    scope: u64,
    enter: Duration,
    exit: Duration,
}

impl<T: Clone + PartialEq> Presence<T> {
    pub(crate) fn new(enter: Duration, exit: Duration, now: Instant) -> Self {
        Self {
            shown: Glide::settled(0., now, ease_out_circ),
            last: None,
            previous: None,
            changed: None,
            scope: 0,
            enter,
            exit,
        }
    }

    /// Once per frame: follow `state` in `scope`. `animate` false snaps
    /// (reduced motion, a window that cannot slide, a test without a
    /// clock). Returns what to draw, `None` once the surface is out.
    pub(crate) fn follow(
        &mut self,
        state: Option<T>,
        scope: u64,
        animate: bool,
        now: Instant,
    ) -> Option<Drawn<T>> {
        if scope != self.scope {
            self.scope = scope;
            self.previous = None;
            self.changed = None;
            self.last = state.clone();
            let at = if state.is_some() { 1. } else { 0. };
            self.shown = Glide::settled(at, now, ease_out_circ);
            return self.last.clone().map(Drawn::settled);
        }
        match state {
            Some(value) => {
                if self.last.as_ref() != Some(&value) {
                    let swaps = animate && self.last.is_some() && self.shown.value(now) > 0.;
                    self.previous = if swaps { self.last.take() } else { None };
                    self.changed = swaps.then_some(now);
                    self.last = Some(value);
                }
                self.shown.go(1., self.enter, animate, now);
            }
            None => {
                self.shown.go(0., self.exit, animate, now);
                if self.shown.value(now) <= 0. {
                    self.last = None;
                    self.previous = None;
                    self.changed = None;
                }
            }
        }
        let value = self.last.clone()?;
        let swap = self
            .changed
            .map_or(1., |changed| progress(elapsed_since(changed, now), SWAP));
        if swap >= 1. {
            self.previous = None;
            self.changed = None;
        }
        Some(Drawn {
            value,
            shown: self.shown.value(now).clamp(0., 1.),
            leaving: state_is_none(&self.shown),
            swap,
            previous: self.previous.clone(),
        })
    }

    /// Whether the surface still moves (the view asks for frames).
    pub(crate) fn moving(&self, now: Instant) -> bool {
        self.shown.animating(now)
            || self
                .changed
                .is_some_and(|changed| elapsed_since(changed, now) < SWAP)
    }
}

fn state_is_none(shown: &Glide) -> bool {
    shown.target() <= 0.
}

/// Which pinned message the pinned bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PinnedShown {
    pub(crate) chat: i64,
    pub(crate) message: i64,
    pub(crate) index: usize,
    pub(crate) count: usize,
}

/// The size a surface was laid out at last frame.
#[derive(Clone, Default)]
pub(crate) struct Measured {
    pub(crate) height: Rc<Cell<Pixels>>,
    pub(crate) width: Rc<Cell<Pixels>>,
}

/// Every surface that comes and goes, with the sizes they were last laid
/// out at. Borrowed for a moment while a view follows its state; never
/// across a call into GPUI.
pub(crate) struct Surfaces {
    /// The status toast of each window (`None`: the main window).
    pub(crate) toasts:
        std::cell::RefCell<std::collections::HashMap<Option<i64>, Presence<super::status_note::StatusNote>>>,
    pub(crate) edit_bar: std::cell::RefCell<Presence<quill::composer::ComposerEdit>>,
    pub(crate) reply_bar: std::cell::RefCell<Presence<quill::composer::ComposerReplyTo>>,
    pub(crate) forward_bar: std::cell::RefCell<Presence<()>>,
    pub(crate) pinned_bar: std::cell::RefCell<Presence<PinnedShown>>,
    pub(crate) media_panel: std::cell::RefCell<Presence<super::media_panel::PanelTab>>,
    /// The "@", heart and poll corner buttons, by `UnreadJumpKind`.
    pub(crate) corners: std::cell::RefCell<[Presence<i32>; 3]>,
    /// How far the corner stack sits above the jump-to-bottom button,
    /// `0..=1` (tdesktop `CornerButtons::updatePositions`).
    pub(crate) corner_lift: std::cell::RefCell<Glide>,
    pub(crate) edit_bar_size: Measured,
    pub(crate) reply_bar_size: Measured,
    pub(crate) forward_bar_size: Measured,
    pub(crate) pinned_bar_size: Measured,
    pub(crate) media_panel_size: Measured,
}

impl Surfaces {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            toasts: Default::default(),
            edit_bar: std::cell::RefCell::new(Presence::new(SLIDE, EXIT, now)),
            reply_bar: std::cell::RefCell::new(Presence::new(SLIDE, EXIT, now)),
            forward_bar: std::cell::RefCell::new(Presence::new(SLIDE, EXIT, now)),
            pinned_bar: std::cell::RefCell::new(Presence::new(SLIDE, EXIT, now)),
            media_panel: std::cell::RefCell::new(Presence::new(ENTER, EXIT, now)),
            corners: std::cell::RefCell::new([
                Presence::new(CORNER, EXIT, now),
                Presence::new(CORNER, EXIT, now),
                Presence::new(CORNER, EXIT, now),
            ]),
            corner_lift: std::cell::RefCell::new(Glide::settled(0., now, ease_out_circ)),
            edit_bar_size: Measured::default(),
            reply_bar_size: Measured::default(),
            forward_bar_size: Measured::default(),
            pinned_bar_size: Measured::default(),
            media_panel_size: Measured::default(),
        }
    }
}

/// The frame's time: the test clock where a test set one, else now.
pub(crate) fn now() -> Instant {
    #[cfg(test)]
    if let Some(now) = TEST_CLOCK.with(Cell::get) {
        return now;
    }
    Instant::now()
}

/// Whether surfaces may move: not with motion reduced, and in a test only
/// on its clock (the wall clock would make it flaky).
pub(crate) fn animate(cx: &App) -> bool {
    #[cfg(test)]
    if TEST_CLOCK.with(Cell::get).is_none() {
        return false;
    }
    !cx.reduce_motion() && !quill::power_saving::reduce_motion_now()
}

#[cfg(test)]
thread_local! {
    static TEST_CLOCK: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Tests on this thread draw surfaces at `now`, and they move.
#[cfg(test)]
pub(crate) fn set_test_clock(now: Option<Instant>) {
    TEST_CLOCK.with(|clock| clock.set(now));
}

/// The edge a sliding surface is attached to: its content stays put
/// there while the rest reveals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attached {
    Top,
    Bottom,
}

/// `content` sliding open to `shown` of its height, attached to `edge`,
/// with its content fading in. The height comes from the last frame
/// (`measured`, kept current by the element itself), `guess` until then.
/// `gap`: the flex gap before this child, folded into the slide so the
/// siblings do not jump when the surface starts. Fully shown, the content
/// is returned as is.
pub(crate) fn slide_open(
    content: AnyElement,
    shown: f32,
    measured: Rc<Cell<Pixels>>,
    guess: Pixels,
    gap: Pixels,
    edge: Attached,
) -> AnyElement {
    let measure = MeasureSize {
        height: Some(measured.clone()),
        width: None,
        child: content,
    };
    if shown >= 1. {
        return measure.into_any_element();
    }
    let full = match measured.get() {
        height if height > px(0.) => height,
        _ => guess,
    };
    let inner = div()
        .absolute()
        .left_0()
        .right_0()
        .opacity(shown)
        .child(measure);
    let inner = match edge {
        Attached::Top => inner.top_0(),
        Attached::Bottom => inner.bottom_0(),
    };
    div()
        .relative()
        .w_full()
        .flex_none()
        .overflow_hidden()
        .h(full * shown)
        .mt(-gap * (1. - shown))
        .child(inner)
        .into_any_element()
}

/// Records its child's laid-out size for the next frame.
pub(crate) struct MeasureSize {
    pub(crate) height: Option<Rc<Cell<Pixels>>>,
    pub(crate) width: Option<Rc<Cell<Pixels>>>,
    pub(crate) child: AnyElement,
}

impl IntoElement for MeasureSize {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MeasureSize {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(height) = &self.height {
            height.set(bounds.size.height.ceil());
        }
        if let Some(width) = &self.width {
            width.set(bounds.size.width.ceil());
        }
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{ENTER, EXIT, Presence, SWAP};
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn appearing_glides_in_and_settles() {
        let t0 = Instant::now();
        let mut p = Presence::new(ENTER, EXIT, t0);
        assert_eq!(p.follow(None::<u8>, 0, true, t0), None);
        let start = p.follow(Some(7), 0, true, t0).unwrap();
        assert_eq!((start.value, start.shown, start.leaving), (7, 0., false));
        assert!(p.moving(t0 + ms(10)));
        let mid = p.follow(Some(7), 0, true, t0 + ms(100)).unwrap();
        assert!(mid.shown > 0. && mid.shown < 1., "{}", mid.shown);
        let done = p.follow(Some(7), 0, true, t0 + ENTER).unwrap();
        assert_eq!(done.shown, 1.);
        assert!(!p.moving(t0 + ENTER));
    }

    #[test]
    fn leaving_keeps_the_last_value_until_it_is_out() {
        let t0 = Instant::now();
        let mut p = Presence::new(ENTER, EXIT, t0);
        p.follow(Some("a"), 0, false, t0);
        let out = p.follow(None, 0, true, t0).unwrap();
        assert_eq!((out.value, out.shown, out.leaving), ("a", 1., true));
        let mid = p.follow(None, 0, true, t0 + ms(60)).unwrap();
        assert_eq!(mid.value, "a");
        assert!(mid.shown > 0. && mid.shown < 1., "{}", mid.shown);
        assert!(mid.leaving);
        assert!(p.moving(t0 + ms(60)));
        assert_eq!(p.follow(None, 0, true, t0 + EXIT), None);
        assert!(!p.moving(t0 + EXIT));
        // Gone for good: nothing lingers.
        assert_eq!(p.follow(None, 0, true, t0 + ms(500)), None);
    }

    #[test]
    fn reappearing_mid_exit_turns_back_from_where_it_is() {
        let t0 = Instant::now();
        let mut p = Presence::new(ENTER, EXIT, t0);
        p.follow(Some(1), 0, false, t0);
        p.follow(None, 0, true, t0);
        let mid = p.follow(None, 0, true, t0 + ms(60)).unwrap().shown;
        let back = p.follow(Some(1), 0, true, t0 + ms(60)).unwrap();
        assert!((back.shown - mid).abs() < 1e-4);
        assert!(!back.leaving);
        assert!(p.follow(Some(1), 0, true, t0 + ms(76)).unwrap().shown > mid);
    }

    #[test]
    fn a_new_value_while_shown_swaps_without_sliding() {
        let t0 = Instant::now();
        let mut p = Presence::new(ENTER, EXIT, t0);
        p.follow(Some(1), 0, false, t0);
        let swap = p.follow(Some(2), 0, true, t0).unwrap();
        assert_eq!((swap.value, swap.shown, swap.swap), (2, 1., 0.));
        assert_eq!(swap.previous, Some(1));
        assert!(p.moving(t0 + ms(10)));
        let mid = p.follow(Some(2), 0, true, t0 + ms(80)).unwrap();
        assert!((mid.swap - 0.5).abs() < 1e-3);
        assert_eq!(mid.previous, Some(1));
        let done = p.follow(Some(2), 0, true, t0 + SWAP).unwrap();
        assert_eq!((done.swap, done.previous), (1., None));
        assert!(!p.moving(t0 + SWAP));
    }

    #[test]
    fn another_scope_snaps() {
        let t0 = Instant::now();
        let mut p = Presence::new(ENTER, EXIT, t0);
        p.follow(Some(1), 11, true, t0);
        // Shown at once in a chat it was never shown in.
        let other = p.follow(Some(5), 12, true, t0).unwrap();
        assert_eq!((other.shown, other.previous), (1., None));
        assert!(!p.moving(t0));
        // Gone at once with the chat.
        assert_eq!(p.follow(None, 13, true, t0), None);
        assert!(!p.moving(t0));
    }

    #[test]
    fn without_animation_it_snaps() {
        let t0 = Instant::now();
        let mut p = Presence::new(ENTER, EXIT, t0);
        let on = p.follow(Some(1), 0, false, t0).unwrap();
        assert_eq!(on.shown, 1.);
        assert!(!p.moving(t0));
        let swap = p.follow(Some(2), 0, false, t0).unwrap();
        assert_eq!((swap.swap, swap.previous), (1., None));
        assert_eq!(p.follow(None, 0, false, t0), None);
    }
}
