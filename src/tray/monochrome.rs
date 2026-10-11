//! tdesktop's "Use monochrome icon" (`trayIconMonochrome`, default on).
//!
//! On Windows `platform/win/tray_win.cpp` swaps the full-color logo for a
//! one-color glyph that matches the taskbar theme (white on a dark taskbar,
//! near-black on a light one, read from `SystemUsesLightTheme`), and marks
//! unread chats with a small dot at the bottom left (`MonochromeWithDot`)
//! instead of the counter. macOS already uses a template image and Linux
//! needs a `-symbolic` icon theme entry Quill does not ship, so neither
//! offers the switch (see [`super::work_mode`]).

use super::{BADGE_RED, ICON_SIZE, TEMPLATE_MASK};
use std::sync::atomic::{AtomicBool, Ordering};

/// The saved switch, mirrored here so the 1s tray timer needs no app handle.
static MONOCHROME: AtomicBool = AtomicBool::new(true);

pub fn set_monochrome(on: bool) {
    MONOCHROME.store(on, Ordering::Relaxed);
}

pub fn monochrome() -> bool {
    MONOCHROME.load(Ordering::Relaxed)
}

/// Dot color for a muted unread count.
const MUTED_DOT: [u8; 4] = [0x88, 0x88, 0x88, 0xFF];

/// Glyph color for a dark or light taskbar, as tdesktop's `MonochromeIconFor`
/// (alpha 228 on light, so it is not pure black).
pub fn glyph_color(dark_taskbar: bool) -> [u8; 4] {
    if dark_taskbar {
        [255, 255, 255, 255]
    } else {
        [0, 0, 0, 228]
    }
}

/// The one-color tray icon: the app glyph in the taskbar's text color and,
/// while chats are unread, a dot at the bottom left (red, gray when every
/// unread chat is muted). Raw RGBA, `ICON_SIZE` square.
pub fn render_monochrome(unread: u32, muted: bool, dark_taskbar: bool) -> (Vec<u8>, u32, u32) {
    let size = ICON_SIZE as usize;
    let [r, g, b, a] = glyph_color(dark_taskbar);
    let mut buf = vec![0u8; size * size * 4];
    for (i, mask) in TEMPLATE_MASK.iter().enumerate() {
        let px = &mut buf[i * 4..i * 4 + 4];
        px[..3].copy_from_slice(&[r, g, b]);
        px[3] = (u16::from(*mask) * u16::from(a) / 255) as u8;
    }
    if unread > 0 {
        let dot = if muted { MUTED_DOT } else { BADGE_RED };
        // tdesktop: cx=3.9, cy=12.7, r=2.2 on a 16 unit grid.
        let unit = size as f32 / 16.0;
        let (cx, cy, radius) = (3.9 * unit, 12.7 * unit, 2.2 * unit);
        for y in 0..size {
            for x in 0..size {
                let dist = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                // One pixel of anti-aliasing at the edge.
                let cover = (radius + 0.5 - dist).clamp(0.0, 1.0);
                if cover > 0.0 {
                    let px = &mut buf[(y * size + x) * 4..(y * size + x) * 4 + 4];
                    px[..3].copy_from_slice(&dot[..3]);
                    px[3] = px[3].max((cover * 255.0) as u8);
                }
            }
        }
    }
    (buf, ICON_SIZE, ICON_SIZE)
}

/// Whether the taskbar is dark: `Some` when the system theme is readable,
/// `None` where it is not (tdesktop `IsDarkTaskbar`; the switch needs it).
#[cfg(target_os = "windows")]
pub fn dark_taskbar() -> Option<bool> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "SystemUsesLightTheme\0".encode_utf16().collect();
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both strings are NUL-terminated and `value`/`size` describe a
    // live DWORD-sized buffer for the duration of the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(value == 0)
}

#[cfg(not(target_os = "windows"))]
pub fn dark_taskbar() -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(buf: &[u8], x: usize, y: usize) -> u8 {
        buf[(y * ICON_SIZE as usize + x) * 4 + 3]
    }

    #[test]
    fn glyph_matches_the_taskbar_theme() {
        let (dark, w, h) = render_monochrome(0, false, true);
        assert_eq!((w, h), (ICON_SIZE, ICON_SIZE));
        let (light, _, _) = render_monochrome(0, false, false);
        let opaque = |buf: &[u8]| {
            buf.as_chunks::<4>()
                .0
                .iter()
                .find(|p| p[3] > 200)
                .map(|p| p[..3].to_vec())
        };
        assert_eq!(opaque(&dark), Some(vec![255, 255, 255]));
        assert_eq!(opaque(&light), Some(vec![0, 0, 0]));
        // Same silhouette in both; the light glyph is the mask at alpha 228.
        for (i, mask) in TEMPLATE_MASK.iter().enumerate() {
            assert_eq!(dark[i * 4 + 3], *mask);
            assert_eq!(u16::from(light[i * 4 + 3]), u16::from(*mask) * 228 / 255,);
        }
    }

    #[test]
    fn unread_adds_a_bottom_left_dot() {
        let (plain, _, _) = render_monochrome(0, false, true);
        let (red, _, _) = render_monochrome(3, false, true);
        let (gray, _, _) = render_monochrome(3, true, true);
        assert_ne!(plain, red);
        assert_ne!(red, gray);
        // The dot center is opaque; the top-right corner stays as it was.
        assert_eq!(alpha(&red, 15, 50), 255);
        assert_eq!(alpha(&red, 60, 3), alpha(&plain, 60, 3));
        let center = (50 * ICON_SIZE as usize + 15) * 4;
        assert_eq!(&red[center..center + 3], &BADGE_RED[..3]);
        assert_eq!(&gray[center..center + 3], &MUTED_DOT[..3]);
    }

    #[test]
    fn switch_is_mirrored() {
        set_monochrome(false);
        assert!(!monochrome());
        set_monochrome(true);
        assert!(monochrome());
    }
}
