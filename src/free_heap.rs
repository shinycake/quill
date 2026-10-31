//! Freed heap pages go back to the system once Quill goes idle.
//!
//! Allocators keep what is freed for reuse: glibc in its arenas, macOS's
//! libmalloc in its tiny, small and medium regions. Quill now and then
//! frees a lot at once: laid-out text the paragraph cache lets go, the
//! messages of chats left, decoded pictures and sticker frames. Each of
//! those notes its bytes here ([`note_freed`]). Once enough was freed, the
//! UI's idle point ([`IdleRelease`]) hands the free pages back
//! ([`release`]) once: never while the user scrolls, drags, types or
//! resizes, and never in the middle of a frame. Animations alone don't put
//! it off: a looping emoji can keep a window drawing for good, so the
//! release then runs right after one of its frames
//! (`docs/decisions/idle-heap-return.md`).

use std::cell::Cell;
use std::time::{Duration, Instant};

/// Freed bytes worth a release: a few long posts' layouts, a closed chat's
/// messages. Less waits for more.
pub const WORTH_RELEASING: usize = 1 << 20;
/// How long a batch worth a release waits, so frees that come together
/// (a chat left, then its pictures trimmed) go back in one release.
pub const AFTER_FREE: Duration = Duration::from_secs(2);
/// How long after the last input (a key, a pointer event, a scroll, a
/// window moved or resized) the release waits.
pub const AFTER_INPUT: Duration = Duration::from_secs(2);
/// No window frame for this long and nothing is drawing: the release runs
/// as soon as it is due. While frames keep coming (an animation, a video),
/// it runs right after one is drawn instead, in the gap before the next.
pub const QUIET: Duration = Duration::from_millis(400);

thread_local! {
    /// Bytes freed on this thread since the last release.
    static FREED: Cell<usize> = const { Cell::new(0) };
    /// Releases made on this thread.
    static RELEASES: Cell<u64> = const { Cell::new(0) };
}

/// `bytes` of heap were freed on this thread.
pub fn note_freed(bytes: usize) {
    // A history dropped as the thread ends notes nothing.
    let _ = FREED.try_with(|freed| freed.set(freed.get().saturating_add(bytes)));
}

/// Bytes freed on this thread since the last release.
pub fn freed() -> usize {
    FREED.with(Cell::get)
}

/// Releases made on this thread so far (tests count them).
pub fn releases() -> u64 {
    RELEASES.with(Cell::get)
}

/// Hand the allocator's free pages back to the system.
/// - macOS: `malloc_zone_pressure_relief` on every zone. Blocks of 64 KiB
///   and up already went back when freed (`quill::large_alloc`).
/// - Linux (glibc): `malloc_trim`.
/// - Windows and other libcs: nothing; the Windows heap decommits large
///   free ranges on its own.
pub fn release() {
    FREED.with(|freed| freed.set(0));
    RELEASES.with(|releases| releases.set(releases.get() + 1));
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // SAFETY: a null zone means "all zones"; goal 0 means "as much as
        // possible" (`<malloc/malloc.h>`). The call only releases free
        // pages.
        unsafe {
            malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
        }
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: glibc's `malloc_trim` only returns free heap memory.
        unsafe {
            libc::malloc_trim(0);
        }
    }
}

/// When to release what was freed: once [`WORTH_RELEASING`] was freed,
/// [`AFTER_FREE`] after that and [`AFTER_INPUT`] after the last input. Then
/// a timer look releases if no window drew for [`QUIET`]; otherwise the
/// next frame does, once it is drawn ([`Self::frame_begins`]). One release
/// covers everything freed before it.
#[derive(Debug, Default)]
pub struct IdleRelease {
    /// When the batch was first seen worth a release.
    ready_at: Option<Instant>,
    /// The last input.
    input_at: Option<Instant>,
    /// Window frames begun, at the last look.
    frames: u64,
    /// When a look last found more frames begun (or the first look).
    drawn_at: Option<Instant>,
}

impl IdleRelease {
    /// Input at `now`: a key, a pointer event, a scroll, a window moved or
    /// resized.
    pub fn input(&mut self, now: Instant) {
        self.input_at = Some(now);
    }

    /// When the release may run, if enough was freed.
    fn due(&mut self, now: Instant) -> Option<Instant> {
        if freed() < WORTH_RELEASING {
            self.ready_at = None;
            return None;
        }
        let ready = *self.ready_at.get_or_insert(now) + AFTER_FREE;
        Some(
            self.input_at
                .map_or(ready, |at| ready.max(at + AFTER_INPUT)),
        )
    }

    /// A timer look at `now`, with `frames` window frames begun so far in
    /// all windows: release if it is due and nothing drew lately. How long
    /// until the next look, or `None` when nothing is left to release.
    pub fn look(&mut self, now: Instant, frames: u64) -> Option<Duration> {
        if frames != self.frames || self.drawn_at.is_none() {
            self.frames = frames;
            self.drawn_at = Some(now);
        }
        let due = self.due(now)?;
        let quiet = self.drawn_at.map_or(now, |at| at + QUIET);
        let wait = due.max(quiet).checked_duration_since(now);
        if let Some(wait) = wait.filter(|wait| !wait.is_zero()) {
            return Some(wait);
        }
        self.frame_drawn();
        None
    }

    /// A window begins a frame at `now`: whether to release once it is
    /// drawn ([`Self::frame_drawn`]), in the gap before the next one.
    pub fn frame_begins(&mut self, now: Instant) -> bool {
        self.due(now).is_some_and(|due| due <= now)
    }

    /// Release what is waiting, if anything: a second frame that asked
    /// finds the batch gone.
    pub fn frame_drawn(&mut self) {
        if freed() >= WORTH_RELEASING {
            release();
        }
        self.ready_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Timer looks every 100 ms from `start` for `secs`, with no frame
    /// drawn; the releases made meanwhile.
    fn idle_looks(idle: &mut IdleRelease, start: Instant, secs: u64) -> u64 {
        let before = releases();
        for step in 0..secs * 10 {
            let _ = idle.look(start + ms(100 * step), idle.frames);
        }
        releases() - before
    }

    #[test]
    fn a_large_free_is_released_once_quiet() {
        let mut idle = IdleRelease::default();
        let start = Instant::now();
        note_freed(WORTH_RELEASING * 20);
        assert_eq!(idle.look(start, 0), Some(AFTER_FREE));
        assert_eq!(idle_looks(&mut idle, start, 10), 1);
        assert_eq!(freed(), 0);
        assert_eq!(idle.look(start + AFTER_FREE * 10, 0), None);
    }

    #[test]
    fn nothing_freed_is_never_released() {
        let mut idle = IdleRelease::default();
        let start = Instant::now();
        note_freed(WORTH_RELEASING - 1);
        assert_eq!(idle.look(start, 0), None);
        assert!(!idle.frame_begins(start + AFTER_FREE * 30));
        assert_eq!(idle_looks(&mut idle, start, 60), 0);
    }

    /// Input every 100 ms for five seconds (a scroll, typing) puts the
    /// release off, frames or not; it runs `AFTER_INPUT` after the last.
    #[test]
    fn input_puts_it_off() {
        let start = Instant::now();
        let mut idle = IdleRelease::default();
        note_freed(WORTH_RELEASING);
        let before = releases();
        for step in 0..50 {
            let now = start + ms(100 * step);
            idle.input(now);
            assert!(!idle.frame_begins(now));
            let _ = idle.look(now + ms(50), 0);
        }
        assert_eq!(releases(), before, "not while input comes");
        // The last input was at 4,900 ms.
        assert_eq!(idle.look(start + ms(4899) + AFTER_INPUT, 0), Some(ms(1)));
        assert_eq!(idle.look(start + ms(4900) + AFTER_INPUT, 0), None);
        assert_eq!(releases(), before + 1);
    }

    /// Frames at 60 Hz with no input (an animation): the timer, which
    /// can't tell where a frame falls, never releases; the first frame
    /// once it is due releases right after it is drawn, and only that one.
    #[test]
    fn while_an_animation_draws_one_frame_releases_after_it_is_drawn() {
        let start = Instant::now();
        let mut idle = IdleRelease::default();
        note_freed(WORTH_RELEASING);
        let before = releases();
        let mut asked = Vec::new();
        for frame in 1..=300 {
            let now = start + ms(16 * frame);
            if idle.frame_begins(now) {
                asked.push(now);
                idle.frame_drawn();
            }
            assert_eq!(idle.look(now + ms(8), frame).is_some(), freed() > 0);
        }
        assert_eq!(asked.len(), 1);
        assert!(asked.first().is_some_and(|at| *at >= start + AFTER_FREE));
        assert_eq!(releases(), before + 1);
        // A second window's frame that also asked finds the batch gone.
        idle.frame_drawn();
        assert_eq!(releases(), before + 1);
    }
}
