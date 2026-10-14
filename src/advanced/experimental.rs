//! The experimental options page (tdesktop `settings/settings_experimental.cpp`).
//!
//! tdesktop lists `base::options` toggles by category. Quill lists the ones
//! that switch behavior Quill really has; the rest of tdesktop's list has
//! nothing to switch here (see `docs/decisions/codex-advanced-bundle.md`).
//! Ids, names and descriptions are tdesktop's, so a code copied from the
//! page names the same option.
//!
//! "Split profile media tab" keeps living in the chat prefs (the shared media
//! gallery reads it from there); "Fast buttons mode" is stored here, in
//! `experimental_options.json`, like tdesktop's `experimental_options.json`.
//! "Disable Touch Bar (macOS only)." is stored there too; its default follows
//! the hardware, as in tdesktop (`Platform::HasTouchBar`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;

pub const SPLIT_MEDIA_ID: &str = "profile-media-tabs-expanded";
pub const FAST_BUTTONS_ID: &str = "fast-buttons-mode";
pub const TOUCHBAR_DISABLED_ID: &str = "touchbar-disabled";

/// Every option id, in page order.
const ALL_IDS: [&str; 3] = [SPLIT_MEDIA_ID, FAST_BUTTONS_ID, TOUCHBAR_DISABLED_ID];

/// One row of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// A group of rows under a subsection title.
#[derive(Debug, Clone, Copy)]
pub struct Category {
    pub title: &'static str,
    pub options: &'static [OptionSpec],
}

pub const SPLIT_MEDIA: OptionSpec = OptionSpec {
    id: SPLIT_MEDIA_ID,
    name: "Split profile media tab into photos and videos.",
    description: "Show separate photo and video tabs in profiles instead of one combined media tab.",
};

pub const FAST_BUTTONS: OptionSpec = OptionSpec {
    id: FAST_BUTTONS_ID,
    name: "Fast buttons mode",
    description: "Trigger inline keyboard buttons by 1-9 keyboard keys.",
};

pub const TOUCHBAR_DISABLED: OptionSpec = OptionSpec {
    id: TOUCHBAR_DISABLED_ID,
    name: "Disable Touch Bar (macOS only).",
    description: "",
};

/// tdesktop's "Profile" category.
pub const PROFILE: Category = Category {
    title: "Profile",
    options: &[SPLIT_MEDIA],
};

/// tdesktop's "Other" category: Fast buttons mode is only listed while it is
/// on, which the `numberbuttons` code does.
pub const OTHER: Category = Category {
    title: "Other",
    options: &[FAST_BUTTONS],
};

/// tdesktop's "Interface" category; of its options only the Touch Bar one
/// switches something in Quill, and only on macOS (tdesktop's `scope`).
pub const INTERFACE: Category = Category {
    title: "Interface",
    options: &[TOUCHBAR_DISABLED],
};

pub const ABOUT: &str = "Warning! These are experimental settings. Some may not work. Others may break the app. Any of them may disappear in the next version without a trace. Use at your own risk.";
pub const RESTORE_LABEL: &str = "Restore default values";
pub const FAST_BUTTONS_ENABLED_TOAST: &str = "Fast buttons mode enabled.";
pub const FAST_BUTTONS_DISABLED_TOAST: &str = "Fast buttons mode disabled.";

/// Every value the page can change. The first two default to off; the
/// Touch Bar is disabled by default on Macs without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Values {
    pub split_profile_media: bool,
    pub fast_buttons_mode: bool,
    pub touchbar_disabled: bool,
}

impl Default for Values {
    fn default() -> Self {
        Self {
            split_profile_media: false,
            fast_buttons_mode: false,
            touchbar_disabled: !has_touch_bar(),
        }
    }
}

/// Whether this Mac has a Touch Bar (tdesktop `Platform::HasTouchBar`):
/// every Intel Mac is assumed to, and of the Apple silicon ones only the
/// 13-inch MacBook Pro models `MacBookPro17,1` and `Mac14,7`. Asked once.
pub fn has_touch_bar() -> bool {
    static HAS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HAS.get_or_init(detect_touch_bar)
}

fn detect_touch_bar() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        matches!(
            hardware_model().as_deref(),
            Some("MacBookPro17,1" | "Mac14,7")
        )
    }
    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// `hw.model`, e.g. "MacBookPro18,3".
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn hardware_model() -> Option<String> {
    let name = c"hw.model";
    let mut length: libc::size_t = 0;
    // SAFETY: a size query: no output buffer, `length` receives the size.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || length == 0 {
        return None;
    }
    let mut bytes = vec![0u8; length];
    // SAFETY: `bytes` holds `length` writable bytes, the size sysctl asked
    // for; it writes at most `length` and updates it.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            bytes.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    bytes.truncate(length);
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    bytes.truncate(end);
    String::from_utf8(bytes).ok()
}

impl Values {
    /// Whether any option differs from its default (tdesktop
    /// `base::options::changed`): the page then offers "Restore default
    /// values".
    pub fn changed(self) -> bool {
        self != Self::default()
    }

    pub fn get(self, id: &str) -> Option<bool> {
        match id {
            SPLIT_MEDIA_ID => Some(self.split_profile_media),
            FAST_BUTTONS_ID => Some(self.fast_buttons_mode),
            TOUCHBAR_DISABLED_ID => Some(self.touchbar_disabled),
            _ => None,
        }
    }

    pub fn set(&mut self, id: &str, value: bool) -> bool {
        match id {
            SPLIT_MEDIA_ID => self.split_profile_media = value,
            FAST_BUTTONS_ID => self.fast_buttons_mode = value,
            TOUCHBAR_DISABLED_ID => self.touchbar_disabled = value,
            _ => return false,
        }
        true
    }
}

const EXPORT_PREFIX: &str = "quill-flags:";

/// The text behind the page's "Export" action: the options that differ from
/// their defaults as JSON, deflated and base64url encoded (tdesktop
/// `EncodeOptionsToText`, with Quill's prefix).
pub fn encode_export(values: Values) -> String {
    use base64::Engine as _;
    use std::io::Write as _;
    let defaults = Values::default();
    let changed: BTreeMap<&str, bool> = ALL_IDS
        .into_iter()
        .filter_map(|id| {
            let value = values.get(id)?;
            (Some(value) != defaults.get(id)).then_some((id, value))
        })
        .collect();
    let json = serde_json::to_vec(&changed).unwrap_or_else(|_| b"{}".to_vec());
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let _ = encoder.write_all(&json);
    let compressed = encoder.finish().unwrap_or_default();
    format!(
        "{EXPORT_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed)
    )
}

/// What "Import" reads from the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Imported {
    /// Not an experimental settings code.
    NotACode,
    /// A code, but its data is not in a supported format.
    Unsupported,
    /// The decoded values: options the code names are set, the rest reset.
    Values(Values),
}

pub fn decode_import(text: &str) -> Imported {
    use base64::Engine as _;
    use std::io::Read as _;
    let Some(encoded) = text.trim().strip_prefix(EXPORT_PREFIX) else {
        return Imported::NotACode;
    };
    let Ok(compressed) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded) else {
        return Imported::NotACode;
    };
    let mut json = Vec::new();
    if flate2::read::ZlibDecoder::new(compressed.as_slice())
        .take(64 * 1024)
        .read_to_end(&mut json)
        .is_err()
    {
        return Imported::NotACode;
    }
    let Ok(parsed) = serde_json::from_slice::<BTreeMap<String, serde_json::Value>>(&json) else {
        return Imported::NotACode;
    };
    let mut values = Values::default();
    for (id, value) in parsed {
        let Some(on) = value.as_bool() else {
            return Imported::Unsupported;
        };
        if !values.set(&id, on) {
            return Imported::Unsupported;
        }
    }
    Imported::Values(values)
}

/// Whether the clipboard text could be imported; the page shows "Import"
/// only then (tdesktop `DecodeOptionsFromText(...).ok`).
pub fn is_importable(text: &str) -> bool {
    !matches!(decode_import(text), Imported::NotACode)
}

pub const IMPORTED_TOAST: &str = "Experimental settings imported from code in clipboard.";
pub const EXPORTED_TOAST: &str = "Experimental settings code copied to clipboard.";
pub const INVALID_CODE_TOAST: &str =
    "Clipboard does not contain a valid experimental settings code.";
pub const UNSUPPORTED_CODE_TOAST: &str =
    "Experimental settings code is valid, but data format is not supported.";

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    #[serde(rename = "fast-buttons-mode")]
    fast_buttons_mode: bool,
    /// Unset: follow the hardware.
    #[serde(rename = "touchbar-disabled", skip_serializing_if = "Option::is_none")]
    touchbar_disabled: Option<bool>,
}

static STORED: RwLock<Option<Stored>> = RwLock::new(None);

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("experimental_options.json"))
}

fn stored() -> Stored {
    if let Ok(guard) = STORED.read()
        && let Some(stored) = guard.as_ref()
    {
        return *stored;
    }
    let loaded = path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .unwrap_or_default();
    if let Ok(mut guard) = STORED.write() {
        guard.get_or_insert(loaded);
    }
    loaded
}

/// Whether Fast buttons mode is switched on (tdesktop `FastButtonsMode()`).
/// The per-bot switch and the number keys only exist while it is.
pub fn fast_buttons_mode() -> bool {
    stored().fast_buttons_mode
}

/// Switch Fast buttons mode and keep it for the next launch.
pub fn set_fast_buttons_mode(on: bool) {
    store(Stored {
        fast_buttons_mode: on,
        ..stored()
    });
}

/// Whether the Touch Bar is switched off (tdesktop `kOptionDisableTouchbar`).
pub fn touchbar_disabled() -> bool {
    stored()
        .touchbar_disabled
        .unwrap_or_else(|| !has_touch_bar())
}

/// Switch the Touch Bar off or back on and keep it for the next launch.
pub fn set_touchbar_disabled(on: bool) {
    store(Stored {
        touchbar_disabled: (on != !has_touch_bar()).then_some(on),
        ..stored()
    });
}

fn store(next: Stored) {
    if crate::fast_buttons::persistence_enabled()
        && let Some(path) = path()
    {
        let _ = crate::settings::write_json_atomic(&path, &next);
    }
    if let Ok(mut guard) = STORED.write() {
        *guard = Some(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_off_and_a_change_is_noticed() {
        let mut values = Values::default();
        assert!(!values.changed());
        assert!(values.set(SPLIT_MEDIA_ID, true));
        assert!(values.changed());
        assert_eq!(values.get(SPLIT_MEDIA_ID), Some(true));
        assert!(!values.set("no-such-option", true));
        assert_eq!(values.get("no-such-option"), None);
    }

    #[test]
    fn an_export_round_trips() {
        let values = Values {
            split_profile_media: true,
            fast_buttons_mode: true,
            touchbar_disabled: has_touch_bar(),
        };
        let text = encode_export(values);
        assert!(text.starts_with("quill-flags:"));
        assert_eq!(decode_import(&text), Imported::Values(values));
        let none = encode_export(Values::default());
        assert_eq!(decode_import(&none), Imported::Values(Values::default()));
    }

    #[test]
    fn only_changed_options_are_exported() {
        let text = encode_export(Values {
            split_profile_media: true,
            ..Values::default()
        });
        let Imported::Values(values) = decode_import(&text) else {
            panic!("export must import");
        };
        assert!(values.split_profile_media && !values.fast_buttons_mode);
    }

    #[test]
    fn the_touch_bar_option_defaults_to_the_hardware() {
        let defaults = Values::default();
        assert_eq!(defaults.touchbar_disabled, !has_touch_bar());
        if !cfg!(target_os = "macos") {
            assert!(!has_touch_bar());
        }
        // Only a change from the hardware default is exported.
        let flipped = Values {
            touchbar_disabled: has_touch_bar(),
            ..defaults
        };
        assert!(flipped.changed());
        let text = encode_export(flipped);
        assert_eq!(decode_import(&text), Imported::Values(flipped));
        assert_eq!(
            decode_import(&encode_export(defaults)),
            Imported::Values(defaults)
        );
    }

    #[test]
    fn foreign_text_is_not_a_code_and_unknown_options_are_unsupported() {
        assert_eq!(decode_import("hello"), Imported::NotACode);
        assert_eq!(decode_import("quill-flags:%%%"), Imported::NotACode);
        assert!(!is_importable("tdesktop-flags:abc"));
        // {"from-the-future":true}, encoded the same way.
        use base64::Engine as _;
        use std::io::Write as _;
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(br#"{"from-the-future":true}"#).unwrap();
        let text = format!(
            "quill-flags:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(enc.finish().unwrap())
        );
        assert_eq!(decode_import(&text), Imported::Unsupported);
        assert!(is_importable(&text));
    }
}
