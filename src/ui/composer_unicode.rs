//! "Insert Unicode control character" in the composer's right-click menu.
//!
//! Telegram Desktop builds its field menu from Qt's standard text menu
//! (`QWidgetTextControl::createStandardContextMenu`), which ends with this
//! submenu when `QStyleHints::useRtlExtensions()` holds; tdesktop only
//! translates its title (`lng_menu_insert_unicode`). Qt sets that hint on
//! Windows when a right-to-left keyboard layout is installed
//! (`QWindowsInputContext::hasRTLInputLanguage`: a layout whose locale's
//! font signature has the right-to-left bit); on macOS and Linux it is never
//! set, so the submenu is Windows-only, as there.
#![deny(clippy::indexing_slicing)]

use super::actions::ComposerInsertControlChar;
use super::app::QuillApp;
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::*;

/// Qt's `qt_controlCharacters`, in its order and wording.
pub(super) const CONTROL_CHARACTERS: [(&str, char); 14] = [
    ("LRM Left-to-right mark", '\u{200e}'),
    ("RLM Right-to-left mark", '\u{200f}'),
    ("ZWJ Zero width joiner", '\u{200d}'),
    ("ZWNJ Zero width non-joiner", '\u{200c}'),
    ("ZWSP Zero width space", '\u{200b}'),
    ("LRE Start of left-to-right embedding", '\u{202a}'),
    ("RLE Start of right-to-left embedding", '\u{202b}'),
    ("LRO Start of left-to-right override", '\u{202d}'),
    ("RLO Start of right-to-left override", '\u{202e}'),
    ("PDF Pop directional formatting", '\u{202c}'),
    ("LRI Left-to-right isolate", '\u{2066}'),
    ("RLI Right-to-left isolate", '\u{2067}'),
    ("FSI First strong isolate", '\u{2068}'),
    ("PDI Pop directional isolate", '\u{2069}'),
];

/// Whether a right-to-left keyboard layout is installed (Qt's
/// `useRtlExtensions` on Windows).
#[cfg(windows)]
fn rtl_extensions() -> bool {
    use windows_sys::Win32::Globalization::{GetLocaleInfoW, LOCALE_FONTSIGNATURE};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayoutList, HKL};
    // SAFETY: a zero-length request with a null buffer only returns the
    // number of layouts.
    let count = unsafe { GetKeyboardLayoutList(0, std::ptr::null_mut()) };
    let Ok(len) = usize::try_from(count) else {
        return false;
    };
    let mut layouts: Vec<HKL> = vec![std::ptr::null_mut(); len];
    // SAFETY: `layouts` has room for `count` handles.
    let got = unsafe { GetKeyboardLayoutList(count, layouts.as_mut_ptr()) };
    let got = usize::try_from(got).unwrap_or(0).min(len);
    layouts.iter().take(got).any(|layout| {
        // The low word of a layout handle is its language id;
        // `MAKELCID(langid, SORT_DEFAULT)` is that id.
        let lcid = (layout.addr() & 0xffff) as u32;
        // `LOCALESIGNATURE` is 32 bytes: 16 UTF-16 units.
        let mut signature = [0u16; 16];
        // SAFETY: the buffer holds the 16 units passed as its length.
        let written =
            unsafe { GetLocaleInfoW(lcid, LOCALE_FONTSIGNATURE, signature.as_mut_ptr(), 16) };
        // `lsUsb[3]` (units 6 and 7), bit 123: right-to-left layout.
        let usb3 = match signature.get(6..8) {
            Some(&[low, high]) => u32::from(low) | (u32::from(high) << 16),
            _ => 0,
        };
        written > 0 && usb3 & 0x0800_0000 != 0
    })
}

#[cfg(not(windows))]
fn rtl_extensions() -> bool {
    false
}

impl QuillApp {
    /// The submenu, after Select All, when Qt would show it.
    pub(super) fn unicode_control_menu(menu: NativeMenu) -> NativeMenu {
        if !rtl_extensions() {
            return menu;
        }
        let mut submenu = NativeMenu::new();
        for (label, character) in CONTROL_CHARACTERS {
            submenu = submenu.menu(label, Box::new(ComposerInsertControlChar { character }));
        }
        menu.separator()
            .submenu("Insert Unicode control character", submenu)
    }

    /// Type the chosen character over the selection.
    pub(super) fn insert_composer_control_char(
        &mut self,
        character: char,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |input, cx| {
            input.replace(character.to_string(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }
}
