//! Composer suggestion demos: a draft with lit words, and an emoji with its
//! sticker row. The keyword and sticker answers are put in the session by hand.

use super::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::{Context, Window};
use quill::ids::{FileId, RequestId};
use quill::telegram::envelope::{StickerFormat, StickerItem};
use quill::telegram::envelope_emoji::EmojiKeyword;

const PUMPKIN: &str = "\u{1f383}";

fn draft(app: &mut QuillApp, text: &str, window: &mut Window, cx: &mut Context<QuillApp>) {
    app.composer.update(cx, |input, cx| {
        input.set_value(text, window, cx);
        input.focus(window, cx);
    });
}

fn answer_keywords(app: &mut QuillApp) {
    let Some(session) = app.demo_session.as_mut() else {
        return;
    };
    let lookups = &mut session.stickers.emoji.keyword_lookups;
    for (word, emoji) in [
        ("pizza", ["🍕", "🧀", "😋"]),
        ("cake", ["🍰", "🎂", "🧁"]),
        ("party", ["🎉", "🥳", "🎊"]),
    ] {
        let id = RequestId(800_000 + lookups.generation);
        lookups.sent(id, word.to_string());
        lookups.answered(
            id,
            emoji
                .iter()
                .map(|e| EmojiKeyword {
                    emoji: e.to_string(),
                    keyword: word.to_string(),
                })
                .collect(),
        );
    }
}

fn sticker(id: i64) -> StickerItem {
    StickerItem {
        custom_emoji_id: None,
        id,
        set_id: 1,
        emoji: PUMPKIN.into(),
        width: 512,
        height: 512,
        format: StickerFormat::Webp,
        file_id: FileId(0),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}

register_demos![
    DemoSpec::chat_list("ready-emoji-word-bar").setup(|app, window, cx| {
        answer_keywords(app);
        draft(
            app,
            "we got pizza and cake for the party tonight with all of our friends \
             and then we keep on writing so the draft wraps onto a second line \
             with cake again near the end of it ",
            window,
            cx,
        );
    }),
    DemoSpec::chat_list("ready-sticker-suggest").setup(|app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let stickers = &mut session.stickers.stickers;
            stickers.suggest_for = Some(PUMPKIN.into());
            stickers.suggestions = (1..=6).map(sticker).collect();
        }
        draft(app, PUMPKIN, window, cx);
    }),
];
