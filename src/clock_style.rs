//! The 12/24-hour clock every time label follows.
//!
//! tdesktop prints times with `QLocale().toString(time, ShortFormat)`, which
//! takes the style from the system: the user's time format on Windows, the
//! "24-hour time" setting on macOS, the `LC_TIME` locale on Linux. Quill
//! reads the same three sources and keeps the answer for a few seconds so
//! a frame that formats a hundred stamps asks the OS once.
//!
//! Precedence: a thread pin (tests), then `QUILL_CLOCK_FORMAT=12|24`, then
//! the system.
#![deny(clippy::indexing_slicing)]

use std::cell::Cell;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::local_time::CivilTime;

/// How long a system answer is reused before the OS is asked again.
const REFRESH: Duration = Duration::from_secs(10);

/// Environment override: `12` or `24`.
pub const ENV_OVERRIDE: &str = "QUILL_CLOCK_FORMAT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockStyle {
    Hour12,
    Hour24,
}

impl ClockStyle {
    /// `21:42` or `9:42 PM` (Qt's `h:mm AP` short format for English).
    pub fn format(self, time: &CivilTime) -> String {
        match self {
            Self::Hour24 => format!("{:02}:{:02}", time.hour, time.minute),
            Self::Hour12 => {
                let hour = match time.hour % 12 {
                    0 => 12,
                    hour => hour,
                };
                let marker = if time.hour >= 12 { "PM" } else { "AM" };
                format!("{hour}:{:02} {marker}", time.minute)
            }
        }
    }

    /// The style in force right now.
    pub fn current() -> Self {
        if let Some(pinned) = PINNED.with(Cell::get) {
            return pinned;
        }
        // The lib's own tests never depend on the machine or its environment.
        if cfg!(test) {
            return Self::Hour24;
        }
        if let Some(forced) = std::env::var(ENV_OVERRIDE)
            .ok()
            .and_then(|value| parse_override(&value))
        {
            return forced;
        }
        cached_system()
    }

    /// Holds this style on the current thread until the guard drops, for
    /// tests that assert literal times (or the 12-hour layout).
    #[must_use]
    pub fn pin(self) -> PinGuard {
        PinGuard {
            previous: PINNED.with(|cell| cell.replace(Some(self))),
        }
    }
}

thread_local! {
    static PINNED: Cell<Option<ClockStyle>> = const { Cell::new(None) };
}

/// Restores the previous pin on drop.
pub struct PinGuard {
    previous: Option<ClockStyle>,
}

impl Drop for PinGuard {
    fn drop(&mut self) {
        PINNED.with(|cell| cell.set(self.previous));
    }
}

fn parse_override(value: &str) -> Option<ClockStyle> {
    match value.trim() {
        "12" => Some(ClockStyle::Hour12),
        "24" => Some(ClockStyle::Hour24),
        _ => None,
    }
}

fn cached_system() -> ClockStyle {
    static CACHE: Mutex<Option<(Instant, ClockStyle)>> = Mutex::new(None);
    let now = Instant::now();
    let Ok(mut slot) = CACHE.lock() else {
        return system();
    };
    if let Some((read_at, style)) = *slot
        && now.duration_since(read_at) < REFRESH
    {
        return style;
    }
    let style = system();
    *slot = Some((now, style));
    style
}

/// The platform's current preference; 24-hour where it cannot be read.
pub fn system() -> ClockStyle {
    if system_is_12_hour().unwrap_or(false) {
        ClockStyle::Hour12
    } else {
        ClockStyle::Hour24
    }
}

/// An ICU or Windows time pattern is 12-hour when it has an unquoted `h`,
/// `K`, `a` (ICU) or `t` (Windows) field; `H` and `k` are 24-hour.
pub fn pattern_is_12_hour(pattern: &str) -> bool {
    let mut quoted = false;
    for ch in pattern.chars() {
        match ch {
            '\'' => quoted = !quoted,
            'h' | 'K' | 'a' | 't' if !quoted => return true,
            _ => {}
        }
    }
    false
}

/// A `strftime` time format is 12-hour when it has `%I`, `%l`, `%p`, `%P`
/// or `%r`.
pub fn strftime_is_12_hour(format: &str) -> bool {
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            continue;
        }
        // Skip glibc flags (`%-I`, `%_l`) before the conversion.
        let mut conv = chars.next();
        while matches!(conv, Some('-' | '_' | '0' | '^' | '#')) {
            conv = chars.next();
        }
        if matches!(conv, Some('I' | 'l' | 'p' | 'P' | 'r')) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn system_is_12_hour() -> Option<bool> {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{CFRelease, kCFAllocatorDefault};
    use core_foundation_sys::date_formatter::CFDateFormatterCreateDateFormatFromTemplate;
    use core_foundation_sys::locale::CFLocaleCopyCurrent;

    let template = CFString::new("j");
    // SAFETY: the template and the locale are live CF objects for the
    // call; the locale is ours (Copy rule) and released right after, the
    // returned pattern (Create rule) is adopted by `CFString`, and a null
    // return maps to `None`.
    let pattern = unsafe {
        let locale = CFLocaleCopyCurrent();
        if locale.is_null() {
            return None;
        }
        let raw = CFDateFormatterCreateDateFormatFromTemplate(
            kCFAllocatorDefault,
            template.as_concrete_TypeRef(),
            0,
            locale,
        );
        CFRelease(locale.cast());
        if raw.is_null() {
            return None;
        }
        CFString::wrap_under_create_rule(raw).to_string()
    };
    Some(pattern_is_12_hour(&pattern))
}

#[cfg(windows)]
fn system_is_12_hour() -> Option<bool> {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_STIMEFORMAT};

    let mut buffer = [0u16; 80];
    // SAFETY: a null name selects the user's locale; the buffer is ours
    // and its length is passed, so the call writes at most that many units.
    let written = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            LOCALE_STIMEFORMAT,
            buffer.as_mut_ptr(),
            buffer.len() as i32,
        )
    };
    let len = usize::try_from(written).ok()?.checked_sub(1)?;
    let pattern = String::from_utf16_lossy(buffer.get(..len)?);
    Some(pattern_is_12_hour(&pattern))
}

#[cfg(target_os = "linux")]
fn system_is_12_hour() -> Option<bool> {
    use std::ffi::CStr;

    // The locale named by LC_ALL / LC_TIME / LANG, built privately so the
    // process-wide locale (which the C library keeps at "C") is untouched.
    // SAFETY: an empty name asks for the environment's locale; a null
    // result (locale not installed) maps to `None`. The returned string is
    // copied before the locale is freed.
    unsafe {
        let locale = libc::newlocale(libc::LC_TIME_MASK, c"".as_ptr(), std::ptr::null_mut());
        if locale.is_null() {
            return None;
        }
        let item = libc::nl_langinfo_l(libc::T_FMT, locale);
        let format = (!item.is_null()).then(|| CStr::from_ptr(item).to_string_lossy().into_owned());
        libc::freelocale(locale);
        format.map(|format| strftime_is_12_hour(&format))
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn system_is_12_hour() -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_time::civil_at;

    // 2026-09-28 21:42 UTC.
    const T: i64 = 1_790_631_720;

    #[test]
    fn both_styles_print_the_same_instant() {
        let time = civil_at(T, 0);
        assert_eq!(ClockStyle::Hour24.format(&time), "21:42");
        assert_eq!(ClockStyle::Hour12.format(&time), "9:42 PM");
    }

    #[test]
    fn midnight_and_noon_read_as_twelve() {
        let midnight = civil_at(T - 21 * 3600 - 42 * 60, 0);
        assert_eq!(ClockStyle::Hour12.format(&midnight), "12:00 AM");
        assert_eq!(ClockStyle::Hour24.format(&midnight), "00:00");
        let noon = civil_at(T - 9 * 3600 - 42 * 60, 0);
        assert_eq!(ClockStyle::Hour12.format(&noon), "12:00 PM");
        let morning = civil_at(T - 12 * 3600, 0);
        assert_eq!(ClockStyle::Hour12.format(&morning), "9:42 AM");
    }

    #[test]
    fn icu_and_windows_patterns_are_told_apart() {
        for twelve in ["h a", "h:mm a", "h:mm:ss tt", "hh:mm tt", "K:mm a"] {
            assert!(pattern_is_12_hour(twelve), "{twelve}");
        }
        for twenty_four in ["HH", "H:mm:ss", "HH.mm", "k:mm", "HH 'h' mm", "HH:mm 'at'"] {
            assert!(!pattern_is_12_hour(twenty_four), "{twenty_four}");
        }
    }

    #[test]
    fn strftime_formats_are_told_apart() {
        for twelve in ["%r", "%I:%M:%S %p", "%l:%M %P", "%-I:%M %p"] {
            assert!(strftime_is_12_hour(twelve), "{twelve}");
        }
        for twenty_four in ["%T", "%H:%M:%S", "%k:%M", "100%%", "%"] {
            assert!(!strftime_is_12_hour(twenty_four), "{twenty_four}");
        }
    }

    #[test]
    fn the_override_accepts_only_twelve_or_twenty_four() {
        assert_eq!(parse_override("12"), Some(ClockStyle::Hour12));
        assert_eq!(parse_override(" 24 "), Some(ClockStyle::Hour24));
        assert_eq!(parse_override("system"), None);
    }

    #[test]
    fn a_pin_wins_and_restores_on_drop() {
        let outer = ClockStyle::Hour12.pin();
        assert_eq!(ClockStyle::current(), ClockStyle::Hour12);
        {
            let _inner = ClockStyle::Hour24.pin();
            assert_eq!(ClockStyle::current(), ClockStyle::Hour24);
        }
        assert_eq!(ClockStyle::current(), ClockStyle::Hour12);
        drop(outer);
    }

    #[test]
    fn the_system_reader_answers_without_crashing() {
        let _ = system();
        let _ = cached_system();
    }
}
