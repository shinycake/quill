//! `ready-chat-look-wallpaper`: the chat look dialog of a private chat with a
//! new wallpaper picked, so its footer offers "Apply for me" and "Apply for
//! me and {user}" (injected, no live Telegram). `QUILL_DEMO_PREMIUM=1` makes
//! the account Premium; otherwise the second button is locked.
//!
//! `ready-chat-wallpaper`: the same chat showing a wallpaper the other person
//! set, delivered as TDLib's `updateChatBackground`.
//! `QUILL_DEMO_WALLPAPER` picks it: `photo` (the default), `pattern`,
//! `gradient`, `freeform` or `fill`; `QUILL_DEMO_DIMMING` is its dimming.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::OwnedEnvelope;
use quill::telegram::envelope::{
    Background, BackgroundFill, BackgroundType, ChatKind, parse_envelope,
};

register_demos![
    DemoSpec::chats(
        "ready-chat-look-wallpaper",
        "screenshot demo — chat wallpaper applied for me or for both (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_chat_look_wallpaper),
    DemoSpec::chats(
        "ready-chat-wallpaper",
        "screenshot demo — a wallpaper the other person set (injected, no live Telegram)"
    )
    // A debug build decodes a photo slowly.
    .timing(6000, 3500)
    .setup(QuillApp::demo_chat_wallpaper),
];

/// The private chat with someone else that has the lowest id.
pub(super) fn demo_private_chat(session: &Session) -> Option<i64> {
    session
        .chats
        .iter()
        .filter(|(_, chat)| {
            matches!(chat.kind, ChatKind::Private { user_id } if Some(user_id.0) != session.my_user_id)
        })
        .map(|(id, _)| *id)
        .min()
}

/// Two plain installed backgrounds, the first picked.
pub(super) fn seed_backgrounds(session: &mut Session) {
    let fill = |id: i64, name: &str, top: u32, bottom: u32| Background {
        id,
        is_default: false,
        is_dark: false,
        name: name.into(),
        file: None,
        thumbnail: None,
        kind: BackgroundType::Fill(BackgroundFill::Gradient {
            top,
            bottom,
            angle: 0,
        }),
    };
    session.chats_state.installed_backgrounds = quill::state::fetch::Fetch::Ready(vec![
        fill(901, "dawn", 0xF6D365, 0xFDA085),
        fill(902, "sea", 0x84FAB0, 0x8FD3F4),
    ]);
}

/// A ready local `file` object.
fn local_file(path: &std::path::Path) -> String {
    format!(
        r#"{{"@type":"file","id":9001,"size":1,"local":{{"@type":"localFile","path":{:?},"can_be_downloaded":true,"is_downloading_completed":true}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u"}}}}"#,
        path.display()
    )
}

/// A painted photo, so nothing but the demo's own pixels is needed.
fn demo_photo(path: &std::path::Path) -> Option<()> {
    let image = image::RgbImage::from_fn(1200, 900, |x, y| {
        let ripple = ((x as f32 / 90.0).sin() + (y as f32 / 70.0).cos()) * 22.0;
        let channel = |base: f32, slope: f32| (base + slope * y as f32 / 900.0 + ripple) as u8;
        image::Rgb([
            channel(232.0, -150.0),
            channel(150.0, -40.0),
            channel(90.0, 80.0),
        ])
    });
    image.save(path).ok()
}

/// The `background` object `kind` asks for, with a string id as TDLib sends.
fn demo_background(kind: &str) -> Option<String> {
    let dir = quill::scratch::process_dir("quill-chat-wallpaper");
    std::fs::create_dir_all(&dir).ok()?;
    let body = match kind {
        "pattern" => {
            let path = dir.join("pattern.svg");
            let dots: String = (0..64)
                .map(|i| {
                    format!(
                        r#"<circle cx="{}" cy="{}" r="14"/>"#,
                        20 + (i % 8) * 60,
                        20 + (i / 8) * 60
                    )
                })
                .collect();
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="480">{dots}</svg>"#
            );
            std::fs::write(&path, svg).ok()?;
            format!(
                r#""document":{{"@type":"document","document":{}}},"type":{{"@type":"backgroundTypePattern","fill":{{"@type":"backgroundFillFreeformGradient","colors":[2041175,4483703,2968404,5587319]}},"intensity":40,"is_inverted":false,"is_moving":false}}"#,
                local_file(&path)
            )
        }
        "gradient" => r#""type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillGradient","top_color":2245734,"bottom_color":6702199,"rotation_angle":45}}"#.to_string(),
        "freeform" => r#""type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillFreeformGradient","colors":[14339308,11578083,14073816,10530000]}}"#.to_string(),
        "fill" => r#""type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillSolid","color":3355494}}"#.to_string(),
        _ => {
            let path = dir.join("photo.png");
            demo_photo(&path)?;
            format!(
                r#""document":{{"@type":"document","document":{}}},"type":{{"@type":"backgroundTypeWallpaper","is_blurred":false,"is_moving":false}}"#,
                local_file(&path)
            )
        }
    };
    Some(format!(
        r#"{{"@type":"background","id":"6012836012836012836","is_default":false,"is_dark":false,"name":"",{body}}}"#
    ))
}

impl QuillApp {
    fn demo_chat_wallpaper(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let kind = std::env::var("QUILL_DEMO_WALLPAPER").unwrap_or_default();
        let dimming = std::env::var("QUILL_DEMO_DIMMING").unwrap_or_else(|_| "0".into());
        let Some(background) = demo_background(&kind) else {
            return;
        };
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = demo_private_chat(session) else {
            return;
        };
        session.open_chat(ChatId(chat_id));
        let json = format!(
            r#"{{"@type":"updateChatBackground","chat_id":{chat_id},"background":{{"@type":"chatBackground","background":{background},"dark_theme_dimming":{dimming}}}}}"#
        );
        let Ok(envelope) = parse_envelope(&json) else {
            return;
        };
        let seq = session.last_seq + 1;
        session.apply(OwnedEnvelope {
            seq,
            client_id: None,
            envelope,
        });
        cx.notify();
    }

    fn demo_chat_look_wallpaper(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let premium = std::env::var("QUILL_DEMO_PREMIUM").is_ok_and(|value| value == "1");
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.payments.premium_option = Some(premium);
        seed_backgrounds(session);
        let Some(chat_id) = demo_private_chat(session) else {
            return;
        };
        session.open_chat(ChatId(chat_id));
        self.open_chat_look_dialog(chat_id, cx);
        if let Some(dialog) = self.dialogs.chat_look_dialog.as_mut() {
            dialog.background = Some(901);
        }
        cx.notify();
    }
}
