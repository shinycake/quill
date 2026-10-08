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

use super::app::QuillApp;
use gpui_kit::*;
use std::time::Duration;

impl QuillApp {
    /// Ask for another frame within `1 / fps` seconds. Animated content
    /// calls this each time it renders; the clock stops after a tick in
    /// which nothing asked.
    #[track_caller]
    pub(super) fn request_animation_tick(&self, fps: u32, cx: &mut Context<Self>) {
        self.request_animation_tick_for(self.slices.current(), fps, cx);
    }

    /// [`Self::request_animation_tick`] for media playing with sound, which
    /// keeps drawing while the window is in the background (tdesktop only
    /// pauses muted GIFs, stickers and loops there).
    #[track_caller]
    pub(super) fn request_media_tick(&self, fps: u32, cx: &mut Context<Self>) {
        self.request_tick(self.slices.current(), fps, true, cx);
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
        if !tick_wanted(self.window_active.get(), plays_sound) {
            return;
        }
        // `QUILL_TRACE_TICKS=1`: log who keeps the clock running (once a
        // second per call site), to hunt idle redraws.
        if trace_ticks() {
            trace_caller(
                std::panic::Location::caller(),
                fps,
                target,
                self.window_active.get(),
            );
        }
        self.animation_demand
            .set(self.animation_demand.get().max(fps.clamp(1, 60)));
        self.animation_targets.borrow_mut().insert(target);
        if plays_sound {
            self.animation_sound.set(true);
        }
        if self.frame_clock_running.get() {
            return;
        }
        self.frame_clock_running.set(true);
        cx.spawn(async move |this, cx| {
            let mut fps = 30;
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs_f64(1.0 / f64::from(fps)))
                    .await;
                let next = this
                    .update(cx, |this, cx| {
                        let demand = this.animation_demand.replace(0);
                        let targets = std::mem::take(&mut *this.animation_targets.borrow_mut());
                        let sound = this.animation_sound.replace(false);
                        if demand > 0 && tick_wanted(this.window_active.get(), sound) {
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
            let _ = this.update(cx, |this, _| this.frame_clock_running.set(false));
        })
        .detach();
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

fn trace_count(what: &'static str, key: String) {
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
        *keys.entry(key).or_default() += 1;
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
}
