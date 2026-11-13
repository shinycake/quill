//! The operating system's accent color (tdesktop `Window::Theme::SystemAccentColor`).
//!
//! - macOS: `NSColor.controlAccentColor`, converted to sRGB.
//! - Windows: `DwmGetColorizationColor`, the colour behind title bars and
//!   the Settings accent. The WinRT `UISettings` accent would need a COM
//!   projection that Quill does not otherwise link.
//! - Linux: the freedesktop portal's `org.freedesktop.appearance`
//!   `accent-color` (read through `gdbus`), then GNOME's
//!   `org.gnome.desktop.interface accent-color` name through `gsettings`
//!   for desktops without the portal key. Desktops that expose neither
//!   report no accent, and the option is hidden.
//!
//! The parsing and the choice between the custom and the system colour are
//! pure and tested here; only the final reads touch the OS.
#![deny(clippy::indexing_slicing)]

/// Pack three 0..=1 components into 0xRRGGBB. `None` outside the range,
/// which is how the portal says "no preference" (it sends values below 0).
pub fn rgb_from_unit(r: f64, g: f64, b: f64) -> Option<u32> {
    let channel = |v: f64| -> Option<u32> {
        (v.is_finite() && (0.0..=1.0).contains(&v)).then(|| (v * 255.0).round() as u32)
    };
    Some((channel(r)? << 16) | (channel(g)? << 8) | channel(b)?)
}

/// 0xAARRGGBB, as `DwmGetColorizationColor` returns it, to 0xRRGGBB.
pub fn rgb_from_dwm(argb: u32) -> u32 {
    argb & 0x00ff_ffff
}

/// The three numbers in `gdbus call` output such as
/// `(<<(0.2078, 0.5176, 0.8941)>>,)`.
pub fn parse_portal_accent(text: &str) -> Option<u32> {
    let numbers: Vec<f64> = text
        .split(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .filter(|token| !token.is_empty())
        .filter_map(|token| token.parse().ok())
        .collect();
    match numbers.as_slice() {
        [r, g, b] => rgb_from_unit(*r, *g, *b),
        _ => None,
    }
}

/// GNOME's named accent colours (`org.gnome.desktop.interface accent-color`,
/// the libadwaita palette) as 0xRRGGBB.
pub fn gnome_accent(name: &str) -> Option<u32> {
    let name = name.trim().trim_matches('\'').trim_matches('"');
    Some(match name {
        "blue" => 0x3584e4,
        "teal" => 0x2190a4,
        "green" => 0x3a944a,
        "yellow" => 0xc88800,
        "orange" => 0xed5b00,
        "red" => 0xe62d42,
        "pink" => 0xd56199,
        "purple" => 0x9141ac,
        "slate" => 0x6f8396,
        _ => return None,
    })
}

/// The accent to apply (0 = the theme's own): the system colour when the
/// option is on and the OS reports one, else the custom choice.
pub fn effective(custom_rgb: u32, use_system: bool, system: Option<u32>) -> u32 {
    match (use_system, system) {
        (true, Some(rgb)) => rgb.max(1),
        _ => custom_rgb,
    }
}

/// The OS accent as 0xRRGGBB (never 0), if the platform reports one.
///
/// macOS and Windows read it directly (cheap calls). Linux runs `gdbus` and
/// `gsettings`, so a lookup never waits for them: this returns the last
/// value a background read stored and starts nothing. [`refresh_then`]
/// starts a read and tells its caller when it has ended; [`pending`] says
/// whether one runs.
pub fn read() -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        crate::locks::lock_or_recover(&LINUX).last()
    }
    #[cfg(not(target_os = "linux"))]
    {
        read_native().map(|rgb| rgb.max(1))
    }
}

/// Starts a background read of the accent (Linux only; the other
/// platforms read in [`read`]) and tells the caller when it has ended.
/// Nothing starts while one runs, and after one failed (neither tool, no
/// accent, or the session bus not up yet) the next one waits
/// [`RETRY_AFTER`], so opening Appearance or focusing the window later
/// reads again without every frame spawning `gdbus`. Returns whether a read
/// started: then `done` runs when it ends, or is dropped if the read thread
/// dies, so a caller that waits on a channel wakes either way. When nothing
/// started (a read already runs, the last one failed moments ago, or this
/// platform reads in [`read`]) `done` is dropped now.
pub fn refresh_then(done: impl FnOnce() + Send + 'static) -> bool {
    #[cfg(target_os = "linux")]
    {
        linux_refresh(Box::new(done))
    }
    #[cfg(not(target_os = "linux"))]
    {
        drop(done);
        false
    }
}

/// Whether a background read is still running (Linux only).
pub fn pending() -> bool {
    #[cfg(target_os = "linux")]
    {
        crate::locks::lock_or_recover(&LINUX).pending(std::time::Instant::now())
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Both queries of a read end within their limit; past twice that the read
/// is lost.
#[cfg(target_os = "linux")]
const READ_LOST_AFTER: std::time::Duration =
    std::time::Duration::from_secs(2 * crate::child_timeout::DESKTOP_QUERY_LIMIT.as_secs() + 1);

/// How long after a read that found nothing the next one may start.
#[cfg(target_os = "linux")]
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

/// Where the background accent read stands; the last value read stays in
/// every stage.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccentRead {
    Idle {
        last: Option<u32>,
    },
    /// Read `read` is running. Its two queries each end within
    /// `DESKTOP_QUERY_LIMIT`; a read still out at `deadline` is taken for
    /// lost and a new one may start. Only `read` settles it: a lost
    /// read's late end changes nothing.
    Reading {
        read: u64,
        last: Option<u32>,
        deadline: std::time::Instant,
    },
    /// A read found nothing; the next may start at `retry_at`.
    Failed {
        last: Option<u32>,
        retry_at: std::time::Instant,
    },
}

#[cfg(target_os = "linux")]
impl AccentRead {
    fn last(&self) -> Option<u32> {
        match self {
            Self::Idle { last } | Self::Reading { last, .. } | Self::Failed { last, .. } => *last,
        }
    }

    /// A read is running at `now` and may still answer: a lost one (past
    /// its deadline) is not waited for.
    fn pending(&self, now: std::time::Instant) -> bool {
        matches!(self, Self::Reading { deadline, .. } if now < *deadline)
    }

    /// A read may start at `now`: move to `Reading` and return its token.
    fn begin(&mut self, now: std::time::Instant) -> Option<u64> {
        let last = match *self {
            Self::Idle { last } => last,
            Self::Failed { last, retry_at } if now >= retry_at => last,
            Self::Reading { last, deadline, .. } if now >= deadline => last,
            Self::Reading { .. } | Self::Failed { .. } => return None,
        };
        let read = NEXT_READ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *self = Self::Reading {
            read,
            last,
            deadline: now + READ_LOST_AFTER,
        };
        Some(read)
    }

    /// Read `read` ended at `now` with `value` (`None`: it found nothing).
    /// Another read's end, or a second end of the same one, changes
    /// nothing.
    fn finish(&mut self, read: u64, value: Option<u32>, now: std::time::Instant) {
        if let Self::Reading {
            read: running,
            last,
            ..
        } = *self
            && running == read
        {
            *self = match value {
                Some(value) => Self::Idle { last: Some(value) },
                None => Self::Failed {
                    last,
                    retry_at: now + RETRY_AFTER,
                },
            };
        }
    }
}

#[cfg(target_os = "linux")]
static NEXT_READ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[cfg(target_os = "linux")]
static LINUX: std::sync::Mutex<AccentRead> = std::sync::Mutex::new(AccentRead::Idle { last: None });

/// The one end of read `.0`. [`ReadDone::finish`] ends it with what it
/// found; a read that panics drops the guard instead, which counts as one
/// that found nothing. Exactly one of the two runs, and only read `.0` is
/// settled by it, so a read started right after this one ended (or after
/// it was taken for lost) is never ended by this one.
#[cfg(target_os = "linux")]
struct ReadDone(u64);

#[cfg(target_os = "linux")]
impl ReadDone {
    fn finish(self, value: Option<u32>) {
        crate::locks::lock_or_recover(&LINUX).finish(self.0, value, std::time::Instant::now());
        std::mem::forget(self);
    }
}

#[cfg(target_os = "linux")]
impl Drop for ReadDone {
    fn drop(&mut self) {
        crate::locks::lock_or_recover(&LINUX).finish(self.0, None, std::time::Instant::now());
    }
}

/// The read thread's body: read, end the read with the value, then tell
/// the caller.
#[cfg(target_os = "linux")]
fn run_read(id: u64, read: impl FnOnce() -> Option<u32>, done: impl FnOnce()) {
    let guard = ReadDone(id);
    guard.finish(read().map(|rgb| rgb.max(1)));
    done();
}

#[cfg(target_os = "linux")]
fn linux_refresh(done: Box<dyn FnOnce() + Send>) -> bool {
    let mut state = crate::locks::lock_or_recover(&LINUX);
    let Some(read) = state.begin(std::time::Instant::now()) else {
        return false;
    };
    let spawned = crate::thread_guard::spawn_guarded("system-accent", move || {
        run_read(read, read_native, done)
    });
    if spawned.is_err() {
        state.finish(read, None, std::time::Instant::now());
        return false;
    }
    true
}

#[cfg(all(target_os = "macos", feature = "ui"))]
fn read_native() -> Option<u32> {
    use objc2_app_kit::{NSColor, NSColorSpace};
    let accent = NSColor::controlAccentColor();
    let srgb = accent.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    rgb_from_unit(
        srgb.redComponent(),
        srgb.greenComponent(),
        srgb.blueComponent(),
    )
}

#[cfg(windows)]
fn read_native() -> Option<u32> {
    use windows_sys::Win32::Graphics::Dwm::DwmGetColorizationColor;
    use windows_sys::core::BOOL;
    let mut color = 0u32;
    let mut opaque: BOOL = 0;
    // SAFETY: both pointers reference live locals the call writes to.
    let result = unsafe { DwmGetColorizationColor(&mut color, &mut opaque) };
    (result >= 0).then(|| rgb_from_dwm(color))
}

#[cfg(target_os = "linux")]
fn read_native() -> Option<u32> {
    let run = |program: &str, args: &[&str]| -> Option<String> {
        crate::child_timeout::stdout_within(
            program,
            args,
            crate::child_timeout::DESKTOP_QUERY_LIMIT,
        )
    };
    run(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "accent-color",
        ],
    )
    .and_then(|text| parse_portal_accent(&text))
    .or_else(|| {
        run(
            "gsettings",
            &["get", "org.gnome.desktop.interface", "accent-color"],
        )
        .and_then(|name| gnome_accent(&name))
    })
}

#[cfg(not(any(all(target_os = "macos", feature = "ui"), windows, target_os = "linux")))]
fn read_native() -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A read that starts the moment the previous one has ended (its
    /// `done` runs, and the main thread begins another) is not ended by
    /// the first: the first ended once, with its value.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_read_started_after_one_ended_is_not_ended_by_it() {
        *crate::locks::lock_or_recover(&LINUX) = AccentRead::Idle { last: None };
        let a = crate::locks::lock_or_recover(&LINUX).begin(std::time::Instant::now());
        assert!(a.is_some());
        let mut b = None;
        run_read(
            a.unwrap_or_default(),
            || Some(0x3584e4),
            || {
                // Read B starts between the first's end and its drop.
                b = crate::locks::lock_or_recover(&LINUX).begin(std::time::Instant::now());
                assert!(b.is_some());
            },
        );
        assert!(
            matches!(
                *crate::locks::lock_or_recover(&LINUX),
                AccentRead::Reading {
                    read,
                    last: Some(0x3584e4),
                    ..
                } if Some(read) == b
            ),
            "B still reads, with A's value kept"
        );
        // Leave the global as other tests expect.
        *crate::locks::lock_or_recover(&LINUX) = AccentRead::Idle { last: None };
    }

    #[test]
    fn unit_components_pack_and_reject_unset() {
        assert_eq!(rgb_from_unit(1.0, 0.0, 0.0), Some(0xff0000));
        assert_eq!(rgb_from_unit(0.2078, 0.5176, 0.8941), Some(0x3584e4));
        assert_eq!(rgb_from_unit(-1.0, -1.0, -1.0), None);
        assert_eq!(rgb_from_unit(0.5, 1.5, 0.5), None);
        assert_eq!(rgb_from_unit(f64::NAN, 0.0, 0.0), None);
    }

    #[test]
    fn dwm_colour_drops_alpha() {
        assert_eq!(rgb_from_dwm(0xc40078d4), 0x0078d4);
    }

    #[test]
    fn portal_output_parses() {
        assert_eq!(
            parse_portal_accent("(<<(0.2078, 0.5176, 0.8941)>>,)\n"),
            Some(0x3584e4)
        );
        assert_eq!(parse_portal_accent("(<(1.0, 0.0, 0.0)>,)"), Some(0xff0000));
        assert_eq!(
            parse_portal_accent("(<<(-1.0, -1.0, -1.0)>>,)"),
            None,
            "the portal's 'no accent' value"
        );
        assert_eq!(parse_portal_accent("(<<(0.1, 0.2)>>,)"), None);
        assert_eq!(parse_portal_accent("Error: no such key"), None);
        assert_eq!(parse_portal_accent(""), None);
    }

    #[test]
    fn gnome_names_map_to_the_palette() {
        assert_eq!(gnome_accent("'blue'\n"), Some(0x3584e4));
        assert_eq!(gnome_accent("'slate'"), Some(0x6f8396));
        assert_eq!(gnome_accent("'unknown'"), None);
        assert_eq!(gnome_accent(""), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_read_that_found_nothing_is_tried_again_after_the_wait() {
        use std::time::Instant;
        let t0 = Instant::now();
        let mut read = AccentRead::Idle { last: None };
        let lost = read.begin(t0).unwrap();
        assert!(read.pending(t0));
        assert!(
            !read.pending(t0 + READ_LOST_AFTER),
            "a lost read is not waited for"
        );
        assert!(read.begin(t0).is_none(), "one read at a time");
        let second = read
            .begin(t0 + READ_LOST_AFTER)
            .expect("a read out past its deadline is taken for lost");
        let t0 = t0 + READ_LOST_AFTER;
        // The lost read answers late: it does not settle the second one.
        read.finish(lost, None, t0);
        assert!(matches!(read, AccentRead::Reading { read: running, .. } if running == second));
        read.finish(second, None, t0);
        assert!(read.begin(t0).is_none(), "not again at once");
        assert!(read.begin(t0 + RETRY_AFTER / 2).is_none());
        let third = read
            .begin(t0 + RETRY_AFTER)
            .expect("a later refresh reads again");
        read.finish(third, Some(0x3584e4), t0 + RETRY_AFTER);
        assert_eq!(read.last(), Some(0x3584e4));
        let fourth = read
            .begin(t0 + RETRY_AFTER)
            .expect("a found accent is read afresh");
        read.finish(fourth, None, t0 + RETRY_AFTER);
        assert_eq!(
            read.last(),
            Some(0x3584e4),
            "the last value survives a miss"
        );
        // A second end of a settled read changes nothing.
        read.finish(fourth, Some(1), t0 + RETRY_AFTER);
        assert_eq!(read.last(), Some(0x3584e4));
    }

    #[test]
    fn system_accent_wins_only_when_chosen_and_available() {
        assert_eq!(effective(0x112233, false, Some(0x445566)), 0x112233);
        assert_eq!(effective(0x112233, true, Some(0x445566)), 0x445566);
        assert_eq!(effective(0x112233, true, None), 0x112233);
        assert_eq!(effective(0, true, None), 0);
        assert_eq!(effective(0, true, Some(0)), 1, "black never means default");
    }
}
