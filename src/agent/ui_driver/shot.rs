//! Screenshots for the UI driver: `take_screenshot`, and `screenshot: true`
//! on the input tools. The UI draws a frame that holds only what the agent
//! may see, reads it back and encodes it (`src/ui/agent_driver_shot.rs`);
//! this is the plain data around it and the rate limit that keeps
//! screenshots a way to operate Quill, not to copy chats out of it.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

/// The longest side of a screenshot unless `full_resolution` is asked for.
pub const MAX_LONG_SIDE: u32 = 1280;
/// At most one screenshot this often, across agents.
pub const MIN_INTERVAL: Duration = Duration::from_secs(1);
/// At most this many screenshots in any minute, across agents.
pub const PER_MINUTE: usize = 20;
/// Whether this build's window can capture itself: the macOS (Metal),
/// Windows (Direct3D 11) and Linux (wgpu on X11 and Wayland) renderers
/// read their frame back; a build without the UI has no window.
pub const CAPTURE_BUILT: bool = cfg!(all(
    feature = "ui",
    any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "freebsd"
    )
));

/// What a caller asks of a screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShotOptions {
    /// Keep the window's device pixels instead of shrinking the long side
    /// to [`MAX_LONG_SIDE`].
    pub full_resolution: bool,
}

/// How much of the window a screenshot shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotScope {
    /// The granted chats and the chat list's own parts; the rest is
    /// painted over.
    Granted,
    /// "All other chats" is on: the whole window but rows of chats that
    /// still need their own grant.
    AllChats,
}

impl ShotScope {
    pub fn as_str(self) -> &'static str {
        match self {
            ShotScope::Granted => "granted",
            ShotScope::AllChats => "all_chats",
        }
    }
}

/// An encoded screenshot.
#[derive(Clone, PartialEq)]
pub struct Shot {
    /// PNG bytes.
    pub png: Vec<u8>,
    /// Image size in pixels.
    pub width: u32,
    pub height: u32,
    /// Image pixels per window logical pixel: a point in the image divided
    /// by it is a point `click_element` takes.
    pub scale: f32,
    pub scope: ShotScope,
}

impl std::fmt::Debug for Shot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shot")
            .field("png_bytes", &self.png.len())
            .field("width", &self.width)
            .field("height", &self.height)
            .field("scale", &self.scale)
            .field("scope", &self.scope)
            .finish()
    }
}

/// The size a `width` x `height` capture is encoded at: as is with
/// `full_resolution`, else shrunk so the long side is at most
/// [`MAX_LONG_SIDE`], keeping the aspect ratio. Never zero.
pub fn output_size(width: u32, height: u32, full_resolution: bool) -> (u32, u32) {
    let (width, height) = (width.max(1), height.max(1));
    let long = width.max(height);
    if full_resolution || long <= MAX_LONG_SIDE {
        return (width, height);
    }
    let shrink = |side: u32| {
        let scaled = u64::from(side) * u64::from(MAX_LONG_SIDE) / u64::from(long);
        u32::try_from(scaled).unwrap_or(MAX_LONG_SIDE).max(1)
    };
    (shrink(width), shrink(height))
}

/// The screenshots taken lately, oldest first, in ms on a caller's clock.
#[derive(Debug, Default)]
pub struct Limiter {
    taken: VecDeque<u64>,
}

impl Limiter {
    /// Take a screenshot at `now_ms` if the limits allow; otherwise how
    /// many ms until one is allowed.
    pub fn reserve(&mut self, now_ms: u64) -> Result<(), u64> {
        let minute = 60_000u64;
        while self
            .taken
            .front()
            .is_some_and(|at| now_ms.saturating_sub(*at) >= minute)
        {
            self.taken.pop_front();
        }
        let interval = MIN_INTERVAL.as_millis() as u64;
        if let Some(last) = self.taken.back() {
            let since = now_ms.saturating_sub(*last);
            if since < interval {
                return Err(interval - since);
            }
        }
        if self.taken.len() >= PER_MINUTE
            && let Some(oldest) = self.taken.front()
        {
            return Err(minute.saturating_sub(now_ms.saturating_sub(*oldest)).max(1));
        }
        self.taken.push_back(now_ms);
        Ok(())
    }
}

static LIMITER: Mutex<Limiter> = Mutex::new(Limiter {
    taken: VecDeque::new(),
});

fn now_ms() -> u64 {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    EPOCH
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
}

/// Count a screenshot against the limits, or say when the next is allowed.
pub fn reserve() -> Result<(), String> {
    crate::locks::lock_or_recover(&LIMITER)
        .reserve(now_ms())
        .map_err(rate_limited)
}

/// The refusal for a screenshot over the limits.
pub fn rate_limited(wait_ms: u64) -> String {
    format!(
        "Screenshots are limited to one a second and {PER_MINUTE} a minute: they are for operating Quill, not for copying chats. Try again in {:.1} s, or use read_ui_tree.",
        wait_ms as f64 / 1000.0
    )
}

/// Forget every screenshot taken (tests).
#[cfg(any(test, feature = "test-support"))]
pub fn reset_limits() {
    crate::locks::lock_or_recover(&LIMITER).taken.clear();
}

#[cfg(test)]
mod tests {
    use super::{Limiter, MAX_LONG_SIDE, PER_MINUTE, output_size, rate_limited};

    #[test]
    fn a_large_capture_shrinks_to_the_long_side_and_keeps_its_shape() {
        assert_eq!(output_size(2200, 1800, false), (MAX_LONG_SIDE, 1047));
        assert_eq!(output_size(1800, 2200, false), (1047, MAX_LONG_SIDE));
        assert_eq!(output_size(2200, 1800, true), (2200, 1800));
        assert_eq!(output_size(1100, 900, false), (1100, 900));
        assert_eq!(output_size(0, 0, false), (1, 1));
        assert_eq!(output_size(100_000, 1, false), (MAX_LONG_SIDE, 1));
    }

    #[test]
    fn one_a_second() {
        let mut limiter = Limiter::default();
        assert_eq!(limiter.reserve(10_000), Ok(()));
        assert_eq!(limiter.reserve(10_400), Err(600));
        assert_eq!(limiter.reserve(11_000), Ok(()));
    }

    #[test]
    fn at_most_so_many_a_minute() {
        let mut limiter = Limiter::default();
        for i in 0..PER_MINUTE as u64 {
            assert_eq!(limiter.reserve(i * 1000), Ok(()), "shot {i}");
        }
        let next = PER_MINUTE as u64 * 1000;
        assert_eq!(limiter.reserve(next), Err(60_000 - next));
        // A minute after the first, one more is allowed.
        assert_eq!(limiter.reserve(60_000), Ok(()));
        assert!(limiter.reserve(61_000).is_ok());
        assert!(rate_limited(1500).contains("1.5 s"));
    }
}
