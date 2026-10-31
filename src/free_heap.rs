//! Freed heap pages go back to the system once Quill goes idle.
//!
//! Allocators keep what is freed for reuse: glibc in its arenas, macOS's
//! libmalloc in its tiny, small and medium regions. Quill now and then
//! frees a lot at once: laid-out text the paragraph cache lets go, the
//! messages of chats left, decoded pictures and sticker frames. Each of
//! those notes its bytes here ([`note_freed`]). Once enough was freed, the
//! UI's idle point ([`IdleRelease`]) hands the free pages back
//! ([`release`]) once, after Quill went quiet: never per frame, never
//! while a window keeps drawing (a scroll, a video) or something animates
//! (`docs/decisions/idle-heap-return.md`).

use std::cell::Cell;
use std::time::{Duration, Instant};

/// Freed bytes worth a release: a few long posts' layouts, a closed chat's
/// messages. Less waits for more.
pub const WORTH_RELEASING: usize = 1 << 20;
/// How long after the last free the release waits, as the image cache's
/// idle trim waits after the last change on screen.
pub const AFTER_FREE: Duration = Duration::from_secs(2);
/// How long no window may have drawn before the release: longer than the
/// gap between frames of anything that moves (a scroll or an animation at
/// 30 fps and up, a playing track's seek bar every 250 ms), shorter than
/// the caret's blink (500 ms), which would otherwise put it off for as
/// long as a field has focus.
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

/// When to release what was freed: once [`WORTH_RELEASING`] was freed, at
/// a look [`AFTER_FREE`] or more after the last look that found more
/// freed, with no window frame begun in the [`QUIET`] before it and
/// nothing animating. One release covers everything freed before it.
#[derive(Debug, Default)]
pub struct IdleRelease {
    /// [`freed`] at the last look.
    seen: usize,
    /// When a look last found more freed.
    freed_at: Option<Instant>,
    /// Window frames begun, at the last look.
    frames: u64,
    /// When a look last found more frames begun (or the first look).
    drawn_at: Option<Instant>,
}

impl IdleRelease {
    /// Look at `now`, with `frames` window frames begun so far in all
    /// windows: release if it is time. How long until the next look, or
    /// `None` when nothing is left to release.
    pub fn look(&mut self, now: Instant, frames: u64, animating: bool) -> Option<Duration> {
        let freed = freed();
        if freed > self.seen {
            self.freed_at = Some(now);
        }
        self.seen = freed;
        if frames != self.frames || self.drawn_at.is_none() {
            self.frames = frames;
            self.drawn_at = Some(now);
        }
        if freed < WORTH_RELEASING {
            return None;
        }
        if animating {
            return Some(AFTER_FREE);
        }
        let due = [
            self.freed_at.map(|at| at + AFTER_FREE),
            self.drawn_at.map(|at| at + QUIET),
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(now);
        if let Some(wait) = due
            .checked_duration_since(now)
            .filter(|wait| !wait.is_zero())
        {
            return Some(wait);
        }
        release();
        self.seen = 0;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Looks every 100 ms from `start` for `secs`, a window frame begun at
    /// each look while `drawing`; the releases made meanwhile.
    fn looks(
        idle: &mut IdleRelease,
        start: Instant,
        secs: u64,
        drawing: bool,
        animating: bool,
    ) -> u64 {
        let before = releases();
        for step in 0..secs * 10 {
            let frames = if drawing { step + 1 } else { 0 };
            let _ = idle.look(start + Duration::from_millis(100 * step), frames, animating);
        }
        releases() - before
    }

    #[test]
    fn a_large_free_is_released_once_quiet() {
        let mut idle = IdleRelease::default();
        let start = Instant::now();
        note_freed(WORTH_RELEASING * 20);
        assert_eq!(idle.look(start, 0, false), Some(AFTER_FREE));
        assert_eq!(looks(&mut idle, start, 10, false, false), 1);
        assert_eq!(freed(), 0);
        assert_eq!(idle.look(start + AFTER_FREE * 10, 0, false), None);
    }

    #[test]
    fn nothing_freed_is_never_released() {
        let mut idle = IdleRelease::default();
        note_freed(WORTH_RELEASING - 1);
        assert_eq!(idle.look(Instant::now(), 0, false), None);
        assert_eq!(looks(&mut idle, Instant::now(), 60, false, false), 0);
    }

    #[test]
    fn a_scroll_or_an_animation_puts_it_off_until_it_ends() {
        let start = Instant::now();
        let mut idle = IdleRelease::default();
        note_freed(WORTH_RELEASING);
        assert_eq!(looks(&mut idle, start, 30, true, false), 0, "drawing");
        let after = start + Duration::from_secs(30);
        assert_eq!(looks(&mut idle, after, 1, false, false), 1, "quiet");

        let mut idle = IdleRelease::default();
        note_freed(WORTH_RELEASING);
        assert_eq!(looks(&mut idle, start, 30, false, true), 0, "animating");
        assert_eq!(looks(&mut idle, after, 3, false, false), 1, "stopped");
    }

    /// The last free sets the time: frees during a scroll keep moving it,
    /// and a frame begun just before a look puts that look off.
    #[test]
    fn it_waits_for_the_last_free_and_a_quiet_moment() {
        let start = Instant::now();
        let mut idle = IdleRelease::default();
        note_freed(WORTH_RELEASING);
        assert_eq!(idle.look(start, 1, false), Some(AFTER_FREE));
        let later = start + Duration::from_secs(1);
        note_freed(1);
        assert_eq!(idle.look(later, 1, false), Some(AFTER_FREE));
        assert_eq!(idle.look(later + AFTER_FREE, 2, false), Some(QUIET));
        assert_eq!(releases(), 0);
        assert_eq!(idle.look(later + AFTER_FREE + QUIET, 2, false), None);
        assert_eq!(releases(), 1);
    }
}
