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
pub fn read() -> Option<u32> {
    read_native().map(|rgb| rgb.max(1))
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
        let output = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
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

#[cfg(not(any(
    all(target_os = "macos", feature = "ui"),
    windows,
    target_os = "linux"
)))]
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
