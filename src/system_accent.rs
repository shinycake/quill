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
    match numbers[..] {
        [r, g, b] => rgb_from_unit(r, g, b),
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
/// value a background read stored and starts nothing. [`refresh`] starts a
/// read; poll [`pending`] to learn when it has finished.
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
/// platforms read in [`read`]). Nothing starts while one runs, and after
/// one failed (neither tool, or no accent) it gives up for good, as the
/// idle-time read in `passcode` does.
pub fn refresh() {
    #[cfg(target_os = "linux")]
    linux_refresh();
}

/// Whether a background read is still running (Linux only).
pub fn pending() -> bool {
    #[cfg(target_os = "linux")]
    {
        matches!(
            *crate::locks::lock_or_recover(&LINUX),
            AccentRead::Reading { .. }
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Where the background accent read stands; the last value read stays in
/// every stage.
#[cfg(target_os = "linux")]
enum AccentRead {
    Idle {
        last: Option<u32>,
    },
    Reading {
        last: Option<u32>,
    },
    /// A read found nothing; never retried.
    Failed {
        last: Option<u32>,
    },
}

#[cfg(target_os = "linux")]
impl AccentRead {
    fn last(&self) -> Option<u32> {
        match self {
            Self::Idle { last } | Self::Reading { last } | Self::Failed { last } => *last,
        }
    }
}

#[cfg(target_os = "linux")]
static LINUX: std::sync::Mutex<AccentRead> = std::sync::Mutex::new(AccentRead::Idle { last: None });

/// Ends a read: a read that panicked counts as failed, so it is not retried
/// every frame. A finished read has already left `Reading`.
#[cfg(target_os = "linux")]
struct ReadDone;

#[cfg(target_os = "linux")]
impl Drop for ReadDone {
    fn drop(&mut self) {
        let mut state = crate::locks::lock_or_recover(&LINUX);
        if let AccentRead::Reading { last } = *state {
            *state = AccentRead::Failed { last };
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_refresh() {
    let mut state = crate::locks::lock_or_recover(&LINUX);
    let AccentRead::Idle { last } = *state else {
        return;
    };
    *state = AccentRead::Reading { last };
    let spawned = crate::thread_guard::spawn_guarded("system-accent", || {
        let _done = ReadDone;
        let value = read_native().map(|rgb| rgb.max(1));
        let mut state = crate::locks::lock_or_recover(&LINUX);
        if let AccentRead::Reading { last } = *state {
            *state = match value {
                Some(value) => AccentRead::Idle { last: Some(value) },
                None => AccentRead::Failed { last },
            };
        }
    });
    if spawned.is_err() {
        *state = AccentRead::Failed { last };
    }
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

    #[test]
    fn system_accent_wins_only_when_chosen_and_available() {
        assert_eq!(effective(0x112233, false, Some(0x445566)), 0x112233);
        assert_eq!(effective(0x112233, true, Some(0x445566)), 0x445566);
        assert_eq!(effective(0x112233, true, None), 0x112233);
        assert_eq!(effective(0, true, None), 0);
        assert_eq!(effective(0, true, Some(0)), 1, "black never means default");
    }
}
