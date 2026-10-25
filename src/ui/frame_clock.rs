//! One redraw clock for everything that animates in the window: custom
//! emoji, animated stickers and inline video. GPUI's own animated images
//! redraw the whole window on every display refresh (120 Hz on ProMotion),
//! relaying out the full tree each time; this clock redraws only as fast
//! as the content needs and stops when nothing animated rendered.
//!
//! Each request is attributed to the cached slice being drawn
//! (`app_slice`); a tick then redraws only the slices that asked, and the
//! rest of the window replays its last frame. Requests made outside any
//! slice redraw the whole app, as before.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::video_player::FrameRequested;
use super::video_player::pacing::{FrameRequest, request_policy};
use gpui_kit::*;
use std::time::{Duration, Instant};

impl QuillApp {
    /// Ask for another frame within `1 / fps` seconds. Animated content
    /// calls this each time it renders; the clock stops after a tick in
    /// which nothing asked.
    #[track_caller]
    pub(super) fn request_animation_tick(&self, fps: u32, cx: &mut Context<Self>) {
        self.request_animation_tick_for(self.slices.current(), fps, cx);
    }

    /// [`Self::request_animation_tick`] for an explicit target: a slice or
    /// the chat list's animation layer by entity id, `None` for the app.
    #[track_caller]
    pub(super) fn request_animation_tick_for(
        &self,
        target: Option<EntityId>,
        fps: u32,
        cx: &mut Context<Self>,
    ) {
        self.request_tick(target, fps, false, cx);
    }

    /// [`Self::request_animation_tick`] for the whole app that keeps
    /// ticking behind another app: the agent takeover, which the user
    /// watches from the app the agent runs in.
    #[track_caller]
    pub(super) fn request_background_animation_tick(&self, fps: u32, cx: &mut Context<Self>) {
        self.request_tick(None, fps, true, cx);
    }

    #[track_caller]
    fn request_tick(
        &self,
        target: Option<EntityId>,
        fps: u32,
        plays_sound: bool,
        cx: &mut Context<Self>,
    ) {
        // Like tdesktop (`isGifPausedAtLeastFor` → `!widget()->isActive()`),
        // nothing animates behind another app; activation redraws and the
        // content asks again (`observe_window_activation`).
        if !tick_wanted(self.frame.window_active.get(), plays_sound) {
            return;
        }
        // `QUILL_TRACE_TICKS=1`: log who keeps the clock running (once a
        // second per call site), to hunt idle redraws.
        if trace_ticks() {
            trace_caller(
                std::panic::Location::caller(),
                fps,
                target,
                self.frame.window_active.get(),
            );
        }
        self.frame
            .animation_demand
            .set(self.frame.animation_demand.get().max(fps.clamp(1, 60)));
        self.frame.animation_targets.borrow_mut().insert(target);
        if plays_sound {
            self.frame.animation_sound.set(true);
        }
        if self.frame.frame_clock_running.get() {
            return;
        }
        self.frame.frame_clock_running.set(true);
        cx.spawn(async move |this, cx| {
            let mut fps = 30;
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs_f64(1.0 / f64::from(fps)))
                    .await;
                let next = this
                    .update(cx, |this, cx| {
                        let demand = this.frame.animation_demand.replace(0);
                        let targets =
                            std::mem::take(&mut *this.frame.animation_targets.borrow_mut());
                        let sound = this.frame.animation_sound.replace(false);
                        if demand > 0 && tick_wanted(this.frame.window_active.get(), sound) {
                            if targets.contains(&None) {
                                cx.notify();
                            } else {
                                // Only the slices with animated content redraw.
                                let app: &mut App = cx;
                                for id in targets.into_iter().flatten() {
                                    app.notify(id);
                                }
                            }
                        }
                        demand
                    })
                    .unwrap_or(0);
                if next == 0 {
                    break;
                }
                fps = next;
            }
            let _ = this.update(cx, |this, _| this.frame.frame_clock_running.set(false));
        })
        .detach();
    }
}

/// What [`request_video_frame`] needs from the playing clip.
pub(crate) struct VideoFrameAsk {
    /// When the next picture becomes due (`None`: unknown, ask every frame).
    pub(crate) next_due: Option<Instant>,
    /// The display's frame interval as measured by the element.
    pub(crate) interval: Duration,
    /// How long before it is shown a frame starts drawing.
    pub(crate) lead: Duration,
    /// The clip plays with sound (keeps drawing behind another app).
    pub(crate) plays_sound: bool,
    /// Keep drawing while the window is inactive (a Picture-in-Picture
    /// window rarely has focus).
    pub(crate) keep_alive: bool,
    /// Shared per player: the request in flight (the view it redraws and
    /// when it counts as lost), so several draws in one frame ask once.
    pub(crate) requested: FrameRequested,
}

/// A request not answered this long after its frame was due is treated as
/// lost (the window closed or the frame never came) and asked again.
const VIDEO_REQUEST_LOST: Duration = Duration::from_millis(250);
/// A sleeping request counts as lost this long after its wake.
const VIDEO_WAKE_SLACK: Duration = Duration::from_millis(50);

/// Whether a request for `target` is already in flight at `now`. One left
/// by another view (a closed Picture-in-Picture window) does not hold this
/// one back.
fn video_request_in_flight(
    requested: Option<(EntityId, Instant)>,
    target: EntityId,
    now: Instant,
) -> bool {
    requested.is_some_and(|(view, lost_at)| view == target && now < lost_at)
}

/// The request for `target` was answered: forget it, unless another view's
/// replaced it meanwhile.
fn video_request_done(requested: &FrameRequested, target: EntityId) {
    if requested.get().is_some_and(|(view, _)| view == target) {
        requested.set(None);
    }
}

/// Ask for the frame that shows the next video picture, redrawing only
/// `target` (the view holding the picture). A frame shows the picture it
/// pulls `lead` after it starts drawing, so the frame for a picture due at
/// `next_due` starts at `next_due - lead`: a one-shot timer wakes half an
/// interval before that and asks for the next display frame. When that is
/// already now, or the due time is unknown, the next display frame is
/// asked for at once (`pacing::request_policy`).
///
/// Unlike [`QuillApp::request_animation_tick`] it runs at the display's own
/// rate, so the picture lands on the vsync it is due for. GPUI still caps
/// an unfocused window at 30 frames a second unless its
/// `inactive_frame_interval` is `None`, as Picture-in-Picture's is.
/// Nothing is asked behind another app unless the clip has sound (or
/// `keep_alive`), like the animation clock.
pub(crate) fn request_video_frame(
    target: EntityId,
    ask: VideoFrameAsk,
    window: &mut Window,
    cx: &mut App,
) {
    #[cfg(test)]
    VIDEO_TARGETS.with(|targets| targets.borrow_mut().push(target));
    let active = window.is_window_active() || assume_active();
    if !tick_wanted(active, ask.plays_sound || ask.keep_alive) {
        return;
    }
    let now = Instant::now();
    if video_request_in_flight(ask.requested.get(), target, now) {
        return;
    }
    let requested = ask.requested;
    match request_policy(now, ask.next_due, ask.interval, ask.lead) {
        FrameRequest::NextFrame => {
            requested.set(Some((
                target,
                now.checked_add(VIDEO_REQUEST_LOST).unwrap_or(now),
            )));
            window.on_next_frame(move |_, cx| {
                video_request_done(&requested, target);
                cx.notify(target);
            });
        }
        FrameRequest::SleepUntil(wake) => {
            let sleep = wake.saturating_duration_since(now);
            // A long sleep (a low-rate clip) must not look lost before it
            // wakes, or a second timer would stack on it.
            let lost_after = VIDEO_REQUEST_LOST.max(sleep.saturating_add(VIDEO_WAKE_SLACK));
            requested.set(Some((target, now.checked_add(lost_after).unwrap_or(now))));
            let handle = window.window_handle();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(sleep).await;
                let asked = handle.update(cx, |_, window, _| {
                    let requested = requested.clone();
                    window.on_next_frame(move |_, cx| {
                        video_request_done(&requested, target);
                        cx.notify(target);
                    });
                });
                if asked.is_err() {
                    video_request_done(&requested, target);
                }
            })
            .detach();
        }
    }
}

/// Whether a frame-clock tick may run: always while the window is active,
/// and in the background only for media playing with sound.
fn tick_wanted(window_active: bool, plays_sound: bool) -> bool {
    window_active || plays_sound
}

/// `QUILL_ASSUME_ACTIVE=1`: animate as if the window had focus, so CPU can
/// be measured on a demo window launched from a terminal (which macOS does
/// not bring to the front).
pub(super) fn assume_active() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_ASSUME_ACTIVE").is_some())
}

#[cfg(test)]
thread_local! {
    /// Every view a video frame was asked for on this thread, in order.
    static VIDEO_TARGETS: std::cell::RefCell<Vec<EntityId>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The views video frames were asked for since the last call, for tests.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn take_video_targets() -> Vec<EntityId> {
    VIDEO_TARGETS.with(|targets| std::mem::take(&mut *targets.borrow_mut()))
}

#[cfg(test)]
thread_local! {
    /// Slice renders on this thread by kind, for tests.
    static SLICE_RENDERS: std::cell::RefCell<std::collections::BTreeMap<&'static str, u32>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// The slice renders since the last call, by kind, for tests.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn take_slice_renders() -> std::collections::BTreeMap<&'static str, u32> {
    SLICE_RENDERS.with(|counts| std::mem::take(&mut *counts.borrow_mut()))
}

/// `QUILL_TRACE_TICKS=1` also counts slice renders (`app_slice`), logged
/// once a second, to check that a tick redraws only what animates.
pub(super) fn trace_slice_render(kind: &'static str) {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::time::Instant;
    thread_local! {
        static COUNTS: RefCell<(Option<Instant>, BTreeMap<&'static str, u32>)> =
            const { RefCell::new((None, BTreeMap::new())) };
    }
    #[cfg(test)]
    SLICE_RENDERS.with(|counts| {
        if let Ok(mut counts) = counts.try_borrow_mut() {
            *counts.entry(kind).or_default() += 1;
        }
    });
    if !trace_ticks() {
        return;
    }
    COUNTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        *counts.1.entry(kind).or_default() += 1;
        let since = *counts.0.get_or_insert_with(Instant::now);
        if since.elapsed() >= Duration::from_secs(1) {
            eprintln!("slice renders/s: {:?}", counts.1);
            counts.1.clear();
            counts.0 = Some(Instant::now());
        }
    });
}

/// `QUILL_TRACE_NOTIFY=1`: log where `QuillApp` notifies come from (the
/// app code that ran the update), once a second per origin with a count.
pub(super) fn trace_notify() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_NOTIFY").is_some())
}

/// Record one `QuillApp` notify. Runs in the notify observer, right after
/// the update that notified; the backtrace still holds that update's
/// caller (a timer task, an event listener, the TDLib poll…).
pub(super) fn trace_notify_origin() {
    if !trace_notify() {
        return;
    }
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    let mut origin: Vec<String> = Vec::new();
    for line in trace.lines().map(str::trim) {
        if line.starts_with("at ") || !line.contains("quill") {
            continue;
        }
        // The innermost Quill function named on this frame, e.g.
        // `<…QuillApp>::spawn_poll_loop::{closure#0}` → `spawn_poll_loop`.
        let name = line
            .rsplit("QuillApp>::")
            .next()
            .filter(|_| line.contains("QuillApp>::"))
            .or_else(|| line.rsplit("quill::").next())
            .unwrap_or(line);
        let name: String = name
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
            .collect();
        let name = name.trim_end_matches(':').to_string();
        if name.is_empty()
            || name.contains("trace_notify")
            || name.contains("notify_slices")
            || name.contains("init_slices")
            || origin.last() == Some(&name)
        {
            continue;
        }
        origin.push(name);
        if origin.len() == 3 {
            break;
        }
    }
    trace_count("notify", origin.join(" <- "));
}

/// `QUILL_TRACE_NOTIFY=1`: count what the TDLib poll applied, by update.
pub(super) fn trace_ingested(update: &str) {
    if trace_notify() {
        trace_count("ingested", update.to_string());
    }
}

/// `QUILL_TRACE_TICKS=1`: count history row rebuilds and chrome patches
/// (`HistoryRow`), and the microseconds spent on them, per second.
pub(super) fn trace_history_rows(what: &'static str, amount: usize) {
    if trace_ticks() {
        trace_count_by("history rows", what.to_string(), amount);
    }
}

fn trace_count(what: &'static str, key: String) {
    trace_count_by(what, key, 1);
}

fn trace_count_by(what: &'static str, key: String, amount: usize) {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::time::Instant;
    /// Per kind of trace: when counting started, and counts by key.
    type Counts = BTreeMap<&'static str, (Instant, BTreeMap<String, u32>)>;
    thread_local! {
        static COUNTS: RefCell<Counts> = const { RefCell::new(BTreeMap::new()) };
    }
    COUNTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        let (since, keys) = counts
            .entry(what)
            .or_insert_with(|| (Instant::now(), BTreeMap::new()));
        let count = keys.entry(key).or_default();
        *count = count.saturating_add(u32::try_from(amount).unwrap_or(u32::MAX));
        if since.elapsed() >= Duration::from_secs(1) {
            let secs = since.elapsed().as_secs_f32();
            for (key, count) in keys.iter() {
                eprintln!("{what}: {:.1}/s {key}", *count as f32 / secs);
            }
            keys.clear();
            *since = Instant::now();
        }
    });
}

pub(super) fn trace_ticks() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_TICKS").is_some())
}

fn trace_caller(
    caller: &'static std::panic::Location<'static>,
    fps: u32,
    target: Option<EntityId>,
    active: bool,
) {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::time::Instant;
    thread_local! {
        static LAST: RefCell<HashMap<(&'static str, u32), Instant>> = RefCell::new(HashMap::new());
    }
    LAST.with(|last| {
        let mut last = last.borrow_mut();
        let key = (caller.file(), caller.line());
        if last
            .get(&key)
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(1))
        {
            last.insert(key, Instant::now());
            let target = target.map_or_else(|| "app".to_string(), |id| format!("slice {id:?}"));
            eprintln!(
                "tick: {}:{} @{fps}fps for {target} (window active: {active})",
                caller.file(),
                caller.line()
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::tick_wanted;

    #[test]
    fn nothing_ticks_behind_another_app() {
        assert!(tick_wanted(true, false));
        assert!(!tick_wanted(false, false));
    }

    #[test]
    fn media_with_sound_keeps_playing_in_the_background() {
        assert!(tick_wanted(false, true));
    }

    #[test]
    fn a_video_request_is_in_flight_for_its_own_view_until_it_is_lost() {
        use super::video_request_in_flight;
        use gpui_kit::EntityId;
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let view = EntityId::from(1u64);
        let other = EntityId::from(2u64);
        let lost_at = now + Duration::from_millis(250);
        assert!(!video_request_in_flight(None, view, now));
        assert!(video_request_in_flight(Some((view, lost_at)), view, now));
        // Left by a closed window: not this view's.
        assert!(!video_request_in_flight(Some((other, lost_at)), view, now));
        // Past its deadline: lost, ask again.
        assert!(!video_request_in_flight(
            Some((view, lost_at)),
            view,
            lost_at + Duration::from_millis(1)
        ));
    }

    /// With the test platform: the request is recorded, held while in
    /// flight, and cleared when its frame comes.
    #[cfg(feature = "demo-capture")]
    #[gpui_kit::test]
    fn a_video_request_clears_when_its_frame_comes(cx: &mut gpui_kit::TestAppContext) {
        use super::{VideoFrameAsk, request_video_frame};
        use crate::ui::video_player::FrameRequested;
        use gpui_kit::{Context, EntityId, IntoElement, ParentElement, Render, Window, div};
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;

        struct Blank;
        impl Render for Blank {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().child("")
            }
        }

        let ask = |requested: &FrameRequested| VideoFrameAsk {
            next_due: None,
            interval: Duration::from_millis(8),
            lead: Duration::from_millis(8),
            plays_sound: true,
            keep_alive: false,
            requested: requested.clone(),
        };
        let (view, vcx) = cx.add_window_view(|_, _| Blank);
        let target = view.entity_id();
        let requested: FrameRequested = Rc::new(Cell::new(None));
        vcx.update(|window, cx| request_video_frame(target, ask(&requested), window, cx));
        let first = requested.get();
        assert!(first.is_some_and(|(view, _)| view == target));
        // A second draw in the same frame asks nothing more.
        vcx.update(|window, cx| request_video_frame(target, ask(&requested), window, cx));
        assert_eq!(requested.get(), first);
        let ran = vcx.update(|window, cx| window.simulate_next_frame(cx));
        assert_eq!(ran, 1, "one frame callback for both draws");
        assert_eq!(requested.get(), None);
        // Another view's request in flight does not hold this one back.
        let other = EntityId::from(u64::MAX);
        requested.set(Some((
            other,
            std::time::Instant::now() + Duration::from_secs(1),
        )));
        vcx.update(|window, cx| request_video_frame(target, ask(&requested), window, cx));
        assert!(requested.get().is_some_and(|(view, _)| view == target));
    }
}
