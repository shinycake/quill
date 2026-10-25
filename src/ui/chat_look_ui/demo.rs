//! `ready-chat-look-wallpaper`: the chat look dialog of a private chat with a
//! new wallpaper picked, so its footer offers "Apply for me" and "Apply for
//! me and {user}" (injected, no live Telegram). `QUILL_DEMO_PREMIUM=1` makes
//! the account Premium; otherwise the second button is locked.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::envelope::{Background, BackgroundFill, BackgroundType, ChatKind};

register_demos![
    DemoSpec::chats(
        "ready-chat-look-wallpaper",
        "screenshot demo — chat wallpaper applied for me or for both (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_chat_look_wallpaper),
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
    session.chats_state.installed_backgrounds = Some(vec![
        fill(901, "dawn", 0xF6D365, 0xFDA085),
        fill(902, "sea", 0x84FAB0, 0x8FD3F4),
    ]);
}

impl QuillApp {
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
