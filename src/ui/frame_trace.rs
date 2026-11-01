//! `QUILL_TRACE_TICKS` and `QUILL_TRACE_NOTIFY`: who keeps the frame clock
//! running and what each tick redraws, logged to hunt idle redraws.
use gpui_kit::EntityId;
use std::time::Duration;

/// `QUILL_TRACE_TICKS=1` also counts slice renders (`app_slice`), logged
/// once a second, to check that a tick redraws only what animates.
pub(in crate::ui) fn trace_slice_render(kind: &'static str) {
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
pub(in crate::ui) fn trace_notify() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_NOTIFY").is_some())
}

/// Record one `QuillApp` notify. Runs in the notify observer, right after
/// the update that notified; the backtrace still holds that update's
/// caller (a timer task, an event listener, the TDLib poll…).
pub(in crate::ui) fn trace_notify_origin() {
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
pub(in crate::ui) fn trace_ingested(update: &str) {
    if trace_notify() {
        trace_count("ingested", update.to_string());
    }
}

/// `QUILL_TRACE_TICKS=1`: count history row rebuilds and chrome patches
/// (`HistoryRow`), and the microseconds spent on them, per second.
pub(in crate::ui) fn trace_history_rows(what: &'static str, amount: usize) {
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

pub(in crate::ui) fn trace_ticks() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_TICKS").is_some())
}

pub(super) fn trace_caller(
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
thread_local! {
    /// Every view a video frame was asked for on this thread, in order.
    pub(super) static VIDEO_TARGETS: std::cell::RefCell<Vec<EntityId>> = const { std::cell::RefCell::new(Vec::new()) };
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
    pub(super) static SLICE_RENDERS: std::cell::RefCell<std::collections::BTreeMap<&'static str, u32>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// The slice renders since the last call, by kind, for tests.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn take_slice_renders() -> std::collections::BTreeMap<&'static str, u32> {
    SLICE_RENDERS.with(|counts| std::mem::take(&mut *counts.borrow_mut()))
}

/// What a window drew over `secs` of the test clock, for tests: the
/// slice renders by kind, with `(window frame)` once per frame (a test
/// window draws as soon as it is dirty), and `(next frame)` for each
/// callback that asked for a display frame (an animation frame), which
/// a test window only runs when asked.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn frames_over(
    vcx: &mut gpui_kit::VisualTestContext,
    secs: u64,
) -> std::collections::BTreeMap<&'static str, u32> {
    let _ = take_slice_renders();
    let mut next_frames = 0;
    for _ in 0..secs * 10 {
        vcx.executor().advance_clock(Duration::from_millis(100));
        vcx.run_until_parked();
        next_frames += vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.run_until_parked();
    }
    let mut drawn = take_slice_renders();
    if next_frames > 0 {
        drawn.insert(
            "(next frame)",
            u32::try_from(next_frames).unwrap_or(u32::MAX),
        );
    }
    drawn
}
