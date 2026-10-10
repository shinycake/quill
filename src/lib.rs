//! Quill core: ordered TDLib envelopes, reducers, and synthetic UI helpers.
//! The GPUI binary lives in `src/main.rs` and is compiled with `--features ui`.

pub mod about;
pub mod account_export;
pub mod album;
pub mod animation;
pub mod auth;
pub mod auto_delete;
pub mod autostart;
pub mod calls;
pub mod chat_export;
pub mod chat_swipe;
pub mod chatlist_archive;
pub mod chatlist_menu;
pub mod chatlist_style;
pub mod checklist;
pub mod community_mode;
pub mod composer;
pub mod connect;
pub mod connect_smoke;
pub mod credentials;
pub mod data_settings;
pub mod deep_link_inbox;
pub mod deep_link_types;
pub mod diagnostics;
pub mod emoji;
pub mod emoji_catalog;
pub mod file_prefs;
pub mod folder_icons;
pub mod folder_limits;
pub mod folder_picker;
pub mod folders;
pub mod force_reply;
pub mod icon_badge;
pub mod ids;
pub mod key_fingerprint;
pub mod layout;
pub mod lifecycle;
pub mod link_handler;
pub mod link_policy;
pub mod local_path;
pub mod local_time;
pub mod marketplace;
pub mod media_session;
pub mod media_tools;
pub mod media_viewer;
pub mod message_menu;
pub mod moderation;
pub mod mute_menu;
pub mod network_usage;
pub mod notify;
pub mod notify_focus;
pub mod passcode;
pub mod peer_badge;
pub mod phone;
pub mod pin_reorder;
pub mod pins;
pub mod platform;
pub mod playback;
pub mod playlist;
pub mod poll;
pub mod premium_hub;
pub mod presence;
pub mod privacy;
pub mod profile_forms;
pub mod proxy;
pub mod request_share;
pub mod rich;
pub mod row_fx;
pub mod schedule;
pub mod search_filters;
pub mod selection_pin;
pub mod send_button;
pub mod service_text;
#[cfg(test)]
mod service_text_tests;
pub mod settings;
pub mod share_box;
pub mod signin;
pub mod single_instance;
pub mod spell_dict;
#[cfg(windows)]
pub mod spell_win;
pub mod spellcheck;
pub mod state;
pub mod sticker_playback;
pub mod sticker_suggest;
pub mod storage_limits;
pub mod stories_strip;
pub mod story_composer;
pub mod story_extras;
pub mod story_page;
pub mod story_restriction;
pub mod story_ring;
pub mod story_viewer;
pub mod subsection_tabs;
pub mod suggest;
pub mod telegram;
pub mod text;
pub mod text_split;
pub mod translate;
pub mod tray;
#[cfg(all(target_os = "macos", feature = "ui"))]
pub mod tray_mac;
#[cfg(all(target_os = "linux", feature = "ui"))]
pub mod tray_sni;
pub mod update_install;
pub mod updater;
pub mod version;
pub mod video;
pub mod video_decode;
pub mod voice;
#[cfg(feature = "ui")]
pub mod voice_input;
#[cfg(feature = "ui")]
pub mod voice_opus;
#[cfg(windows)]
pub mod winreg;

use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub fn verify_schema_file(schema_path: &Path) -> Result<(), SchemaError> {
    let bytes = fs::read(schema_path).map_err(|_| SchemaError::Missing)?;
    if bytes.len() != pins::TD_API_TL_BYTES {
        return Err(SchemaError::Length {
            got: bytes.len(),
            expected: pins::TD_API_TL_BYTES,
        });
    }
    let digest = hex_sha256(&bytes);
    if digest != pins::TD_API_TL_SHA256 {
        return Err(SchemaError::Checksum);
    }
    let text = String::from_utf8_lossy(&bytes);
    if !text.contains("vector<") {
        return Err(SchemaError::TruncatedVectors);
    }
    for name in pins::REQUIRED_SCHEMA_CONSTRUCTORS {
        let found = text.lines().any(|line| {
            let line = line.trim();
            line == *name
                || line.starts_with(&format!("{name} "))
                || line.starts_with(&format!("{name}="))
        });
        if !found {
            return Err(SchemaError::MissingConstructor(name));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaError {
    Missing,
    Checksum,
    TruncatedVectors,
    Length { got: usize, expected: usize },
    MissingConstructor(&'static str),
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn sha256_empty() {
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn vendored_schema_equals_official_commit() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(pins::TD_API_TL_PATH);
        let vendored = fs::read(&path).expect("vendored schema");
        let official = fetch_official_schema();
        assert_eq!(
            official.len(),
            pins::TD_API_TL_BYTES,
            "official schema size drifted from pin"
        );
        assert_eq!(
            hex_sha256(&official),
            pins::TD_API_TL_SHA256,
            "official schema digest drifted from pin"
        );
        assert_eq!(
            vendored,
            official,
            "vendored schema/td_api.tl must be byte-identical to official td_api.tl at {}",
            pins::TDLIB_GIT_COMMIT
        );
        assert_eq!(verify_schema_file(&path), Ok(()));
        assert!(
            std::str::from_utf8(&official).unwrap().contains("vector<"),
            "official schema must keep typed vector<T> parameters"
        );
    }

    fn fetch_official_schema() -> Vec<u8> {
        let output = Command::new("curl")
            .args([
                "-fsSL",
                "--retry",
                "4",
                "--retry-delay",
                "2",
                pins::TD_API_TL_UPSTREAM_URL,
            ])
            .output()
            .expect("spawn curl to fetch official td_api.tl");
        assert!(
            output.status.success(),
            "failed to fetch {}: {}",
            pins::TD_API_TL_UPSTREAM_URL,
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
}

/// The bidi geometry of the vendored input engine (`third_party/gpui-base`,
/// docs/decisions/codex-rtl-composer.md) is pure and has its own unit tests;
/// compiling the module here runs them with the rest of `cargo test`.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../third_party/gpui-base/src/input/editor/display_map/bidi.rs"]
mod vendored_input_bidi;
