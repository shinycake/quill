//! Screenshot fixtures for per-chat wallpapers and themes (injected, no live
//! Telegram): installed wallpapers including a pattern, four emoji chat
//! themes, and chat 11 wearing one of them.

use super::demo::{demo_file_json, demo_media_allowlist, seed_ready_chats_session};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// Which screen the fixture is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChatLookDemo {
    /// The picker open over the chat.
    Picker,
    /// The chat wearing a theme and its own pattern wallpaper.
    Themed,
    /// A `bg/` link preview.
    Link,
}

fn pattern_path() -> String {
    demo_media_allowlist()
        .join("demo-pattern.svg")
        .to_string_lossy()
        .into_owned()
}

fn fill(top: u32, bottom: u32, angle: i32) -> String {
    format!(
        r#"{{"@type":"backgroundFillGradient","top_color":{top},"bottom_color":{bottom},"rotation_angle":{angle}}}"#
    )
}

fn solid(color: u32) -> String {
    format!(r#"{{"@type":"backgroundFillSolid","color":{color}}}"#)
}

fn fill_background(id: i64, name: &str, fill: &str) -> String {
    format!(
        r#"{{"@type":"background","id":{id},"name":"{name}","type":{{"@type":"backgroundTypeFill","fill":{fill}}}}}"#
    )
}

fn pattern_background(id: i64, name: &str, fill: &str, intensity: i32, inverted: bool) -> String {
    format!(
        r#"{{"@type":"background","id":{id},"name":"{name}","document":{{"@type":"document","document":{file}}},"type":{{"@type":"backgroundTypePattern","fill":{fill},"intensity":{intensity},"is_inverted":{inverted},"is_moving":false}}}}"#,
        file = demo_file_json(9001, &pattern_path(), true),
    )
}

fn theme_settings(accent: u32, outgoing: &str, background: &str) -> String {
    format!(
        r#"{{"@type":"themeSettings","accent_color":{accent},"background":{background},"outgoing_message_fill":{outgoing},"has_outgoing_message_accent_color":false}}"#
    )
}

fn emoji_theme(name: &str, accent: u32, light: (&str, String), dark: (&str, String)) -> String {
    format!(
        r#"{{"@type":"emojiChatTheme","name":"{name}","light_settings":{light},"dark_settings":{dark}}}"#,
        light = theme_settings(accent, light.0, &light.1),
        dark = theme_settings(accent, dark.0, &dark.1),
    )
}

pub(super) fn seed_chat_look_session(sink: Arc<MemorySink>, demo: ChatLookDemo) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    let seq = AtomicU64::new(session.last_seq);
    let apply = |session: &mut Session, json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };

    // Installed wallpapers: fills, gradients and one pattern.
    let doodles = pattern_background(6, "Doodles", &fill(0x8e6cc4, 0x2f8ac9, 45), 45, false);
    let installed = [
        fill_background(1, "Mint", &solid(0xb8e0c9)),
        fill_background(2, "Dusk", &fill(0x2b3a67, 0xb56576, 0)),
        fill_background(3, "Sunrise", &fill(0xffd89b, 0x19547b, 45)),
        fill_background(4, "Sand", &solid(0xe9dcc3)),
        doodles.clone(),
        pattern_background(7, "Night garden", &fill(0x1b2a41, 0x324a5f, 0), 60, true),
    ]
    .join(",");
    let extra = session.request(RequestPurpose::GetInstalledBackgrounds, None);
    apply(
        &mut session,
        &format!(
            r#"{{"@type":"backgrounds","@extra":"{}","backgrounds":[{installed}]}}"#,
            extra.0
        ),
    );

    // Emoji chat themes: light and dark variants with their own backgrounds.
    let themes = [
        emoji_theme(
            "🌷",
            0xd0527d,
            (
                &solid(0xd0527d),
                pattern_background(11, "tulip-light", &fill(0xffe0e9, 0xf6b8cc, 45), 45, false),
            ),
            (
                &solid(0xa83a63),
                pattern_background(12, "tulip-dark", &fill(0x3a1f2c, 0x5d2a45, 45), 50, true),
            ),
        ),
        emoji_theme(
            "🌊",
            0x2f8ac9,
            (
                &fill(0x2f8ac9, 0x1f6eb0, 0),
                fill_background(13, "sea-light", &fill(0xcdeafc, 0x8fc7ee, 0)),
            ),
            (
                &fill(0x246fa8, 0x174e7a, 0),
                fill_background(14, "sea-dark", &fill(0x0c2233, 0x14405e, 0)),
            ),
        ),
        emoji_theme(
            "🍃",
            0x4a9d6a,
            (
                &solid(0x4a9d6a),
                fill_background(15, "leaf-light", &fill(0xdff3d4, 0xa8d8a0, 45)),
            ),
            (
                &solid(0x3b7d55),
                fill_background(16, "leaf-dark", &fill(0x14261a, 0x23402e, 45)),
            ),
        ),
        emoji_theme(
            "🔥",
            0xe0702f,
            (
                &solid(0xe0702f),
                fill_background(17, "fire-light", &fill(0xffe2c4, 0xf4ad7b, 0)),
            ),
            (
                &solid(0xc2571c),
                fill_background(18, "fire-dark", &fill(0x2e1a10, 0x4d2a14, 0)),
            ),
        ),
    ]
    .join(",");
    apply(
        &mut session,
        &format!(r#"{{"@type":"updateEmojiChatThemes","chat_themes":[{themes}]}}"#),
    );

    match demo {
        ChatLookDemo::Picker => {
            apply(
                &mut session,
                r#"{"@type":"updateChatTheme","chat_id":11,"theme":{"@type":"chatThemeEmoji","name":"🌊"}}"#,
            );
        }
        ChatLookDemo::Themed => {
            apply(
                &mut session,
                r#"{"@type":"updateChatTheme","chat_id":11,"theme":{"@type":"chatThemeEmoji","name":"🌷"}}"#,
            );
            apply(
                &mut session,
                &format!(
                    r#"{{"@type":"updateChatBackground","chat_id":11,"background":{{"@type":"chatBackground","dark_theme_dimming":20,"background":{doodles}}}}}"#
                ),
            );
        }
        ChatLookDemo::Link => {
            let extra = session.request(RequestPurpose::SearchBackground, None);
            let mut found: serde_json::Value = serde_json::from_str(&doodles).unwrap_or_default();
            found["@extra"] = serde_json::json!(extra.as_extra());
            apply(&mut session, &found.to_string());
        }
    }
    session
}

pub(super) fn seed_chat_look_picker(sink: Arc<MemorySink>) -> Session {
    seed_chat_look_session(sink, ChatLookDemo::Picker)
}

pub(super) fn seed_chat_look_themed(sink: Arc<MemorySink>) -> Session {
    seed_chat_look_session(sink, ChatLookDemo::Themed)
}

pub(super) fn seed_chat_look_link(sink: Arc<MemorySink>) -> Session {
    seed_chat_look_session(sink, ChatLookDemo::Link)
}
