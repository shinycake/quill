//! Quill core: ordered TDLib envelopes, reducers, and synthetic UI helpers.
//! The GPUI binary lives in `src/main.rs` and is compiled with `--features ui`.

pub mod about;
pub mod add_special;
pub mod admin_extras;
pub mod advanced;
pub mod agent;
pub mod album;
pub mod animation;
pub mod app_theme;
pub mod auth;
pub mod auto_delete;
pub mod autoscroll;
pub mod autostart;
pub mod battery_saving;
pub mod boost_box;
pub mod bot_invite;
pub mod bot_profile;
pub mod bot_verification;
pub mod bubble_layout;
pub mod business_bot_bar;
pub mod business_bots;
pub mod business_chat_links;
pub mod business_hours_edit;
pub mod business_info;
pub mod business_settings;
pub mod business_start_page;
pub mod call_device_prefs;
pub mod call_link;
pub mod calls;
pub mod capture_choice;
pub mod chat_appearance;
pub mod chat_bottom_bar;
pub mod chat_header_menu;
pub mod chat_swipe;
pub mod chat_wide;
pub mod chatlist_archive;
pub mod chatlist_calls;
pub mod chatlist_menu;
pub mod chatlist_style;
pub mod chatlist_suggestions;
pub mod checklist;
pub mod child_reap;
pub mod child_timeout;
pub mod clear_history;
pub mod clock_style;
pub mod code_language;
pub mod collectible_info;
pub mod community_mode;
pub mod composer;
pub mod composer_doc;
pub mod connect;
pub mod connect_smoke;
pub mod contacts_index;
pub mod corner_buttons;
pub mod country_select;
pub mod cpal_stream;
pub mod crash_log;
pub mod credentials;
pub mod data_settings;
pub mod deep_link_inbox;
pub mod deep_link_types;
pub mod dev_cli;
pub mod diagnostics;
pub mod drop_modes;
pub mod drop_topic;
pub mod earn;
pub mod emoji;
pub mod emoji_catalog;
pub mod emoji_interaction;
pub mod emoji_replace;
pub mod emoji_sets_style;
pub mod emoji_status_selector;
pub mod event_log_filter;
pub mod event_log_list;
pub mod export;
pub mod fast_buttons;
pub mod file_prefs;
pub mod folder_archive;
pub mod folder_icons;
pub mod folder_limits;
pub mod folder_picker;
pub mod folders;
pub mod font_choice;
pub mod force_reply;
pub mod forward_drag;
pub mod game_share;
pub mod gif_grid;
pub mod gift_colors;
pub mod gift_manage;
pub mod gift_market;
pub mod giveaway;
pub mod go_to_message;
pub mod group_info_edit;
pub mod group_manage;
pub mod group_type;
pub mod icon_badge;
pub mod ids;
pub mod inline_grid;
pub mod inline_members;
pub mod invite_link_form;
pub mod json_access;
pub mod key_fingerprint;
pub mod lang;
pub mod large_alloc;
pub mod layout;
pub mod lifecycle;
pub mod limit_steps;
pub mod link_handler;
pub mod link_policy;
pub mod live_story;
pub mod local_path;
pub mod local_time;
pub mod location_picker;
pub mod locks;
pub mod main_menu;
pub mod marketplace;
pub mod media_bubble;
pub mod media_capture;
pub mod media_session;
pub mod media_spoiler;
pub mod media_timestamp;
pub mod media_tools;
pub mod media_viewer;
pub mod message_a11y;
pub mod message_effect_watch;
pub mod message_effects;
pub mod message_effects_picker;
pub mod message_menu;
pub mod message_menu_origin;
pub mod moderation;
pub mod moderation_exceptions;
pub mod mute_menu;
pub mod native_crash;
pub mod network_usage;
pub mod new_chat;
pub mod notify;
pub mod notify_accounts;
pub mod notify_call;
pub mod notify_focus;
pub mod notify_prefs;
pub mod notify_volume;
pub mod owned_bots;
pub mod paid_messages;
pub mod paid_messages_guard;
pub mod paid_reaction;
pub mod passcode;
pub mod passkey_probe;
pub mod passkeys;
pub mod payment_card;
pub mod payment_invoice;
pub mod peer_badge;
pub mod peer_color;
pub mod peer_qr;
pub mod permissions_box;
pub mod personal_channel;
pub mod phone;
pub mod phone_suggestion;
pub mod pin_reorder;
pub mod pins;
pub mod pipe_security;
pub mod platform;
pub mod playback;
pub mod playlist;
pub mod poll;
pub mod power_saving;
pub mod premium_hub;
pub mod premium_limits;
pub mod presence;
pub mod privacy;
pub mod profile_actions;
pub mod profile_cover;
pub mod profile_forms;
pub mod profile_photo_camera;
pub mod profile_photo_gradient;
pub mod profile_photo_sources;
pub mod profile_photo_video;
pub mod profile_tab;
pub mod promotion;
pub mod proxy;
pub mod proxy_carrier;
pub mod proxy_extras;
pub mod proxy_sync;
pub mod proxy_web;
pub mod psa_about;
pub mod pull_next;
pub mod quick_action;
pub mod quick_replies;
pub mod quote_source;
pub mod reaction_who;
pub mod release_notes;
pub mod reply_options;
pub mod request_share;
pub mod rich;
pub mod rich_html;
pub mod row_fx;
pub mod schedule;
#[doc(hidden)]
pub mod scratch;
pub mod search_filters;
pub mod search_tabs;
pub mod selection_drag;
pub mod selection_pin;
pub mod send_button;
pub mod send_rights;
pub mod send_to_menu;
pub mod service_text;
#[cfg(test)]
mod service_text_tests;
pub mod settings;
pub mod settings_account;
pub mod settings_catalog;
pub mod share_box;
pub mod signin;
pub mod single_instance;
pub mod spell_catalog;
pub mod spell_dict;
pub mod spell_download;
#[cfg(windows)]
pub mod spell_win;
pub mod spellcheck;
pub mod star_packs;
pub mod stars_gift;
pub mod stars_rating;
pub mod start_files;
pub mod startup_trace;
pub mod stat_chart;
pub mod state;
pub mod sticker_image;
pub mod sticker_playback;
pub mod sticker_set_box;
pub mod sticker_suggest;
pub mod sticker_video;
pub mod stickers_manager;
pub mod storage_limits;
pub mod stories_menu;
pub mod stories_pull;
pub mod stories_strip;
pub mod story_album_picker;
pub mod story_composer;
pub mod story_cover;
pub mod story_extras;
pub mod story_header;
pub mod story_page;
pub mod story_preload;
pub mod story_repost;
pub mod story_restriction;
pub mod story_ring;
pub mod story_sibling_layout;
pub mod story_tray_walk;
pub mod story_viewer;
pub mod subsection_tabs;
pub mod suggest;
pub mod suggested_posts;
pub mod system_accent;
pub mod system_proxy;
pub mod system_text_replace;
pub mod telegram;
pub mod text;
pub mod text_split;
pub mod theme_colorize;
pub mod thread_guard;
pub mod ton_wallet;
pub mod tone_upload;
pub mod top_people;
pub mod topic_info;
pub mod translate;
pub mod tray;
#[cfg(all(target_os = "macos", feature = "ui"))]
pub mod tray_mac;
#[cfg(all(target_os = "linux", feature = "ui"))]
pub mod tray_sni;
pub mod update_install;
pub mod updater;
pub mod vcard_import;
pub mod version;
pub mod video;
pub mod video_decode;
pub mod video_edit;
pub mod video_encoder;
pub mod video_timeline;
pub mod viewer_extras;
pub mod voice;
#[cfg(feature = "ui")]
pub mod voice_input;
#[cfg(feature = "ui")]
pub mod voice_opus;
pub mod wake;
pub mod wallpaper_raster;
pub mod web_app;
pub mod webauthn;
pub mod window_title;
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
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            hex_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

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
