//! `ready-message-effects`: the demo chat with messages sent with effects
//! and the account's effects described (injected, no live Telegram). The
//! fixtures' sticker plays every effect; the thumbnail is every icon.
//!
//! `QUILL_DEMO_EFFECTS` picks what is open over it: `badge` (nothing, the default),
//! `arrive` (an unread message sent with an effect arrives and plays it),
//! `window` (the same, with the chat open in a window of its own; capture
//! it with `QUILL_DEMO_CAPTURE_WINDOWS=1`),
//! `strip` (the send menu), `expanded` (the full list),
//! `card` (the "Send with Effect" card) or `premium` (the card of a Premium
//! effect, offering Premium).

use super::super::app::QuillApp;
use super::super::demo::{demo_file_json, demo_media_allowlist, seed_ready_chats_session};
use super::super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-message-effects",
        seed_message_effects_session,
        "screenshot demo — message effects: badge, send menu picker, preview card (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_message_effects),
];

/// The reaction effects: id, emoji, Premium.
pub(super) const REACTION_EFFECTS: [(i64, &str, bool); 9] = [
    (5_104_841_245_755_180_586, "🔥", false),
    (5_107_584_321_108_051_014, "👍", false),
    (5_044_134_455_711_629_726, "❤", false),
    (5_046_509_860_389_126_442, "🎉", false),
    (5_104_858_069_142_078_462, "👎", false),
    (5_046_589_136_895_476_101, "💩", false),
    (5_159_385_139_981_059_251, "😁", true),
    (5_066_970_843_586_925_436, "🤯", true),
    (5_070_445_174_117_883_018, "✨", true),
];

/// The sticker effects: id, emoji, Premium.
pub(super) const STICKER_EFFECTS: [(i64, &str, bool); 2] = [
    (5_100_000_000_000_000_001, "😀", true),
    (5_100_000_000_000_000_002, "😎", true),
];

/// The demo chat (private, so it takes effects).
pub(super) const CHAT: i64 = 11;

const ICON_FILE: i32 = 9_101;
const ANIMATION_FILE: i32 = 9_102;

fn sticker_json(file: i32, emoji: &str, format: &str, premium_animation: Option<i32>) -> String {
    let file_json = |id: i32, name: &str| {
        demo_file_json(
            id,
            &demo_media_allowlist().join(name).to_string_lossy(),
            true,
        )
    };
    let premium = premium_animation.map_or_else(
        || "null".to_string(),
        |id| file_json(id, "demo-sticker.tgs"),
    );
    let name = if format == "stickerFormatWebp" {
        "demo-thumb.png"
    } else {
        "demo-sticker.tgs"
    };
    format!(
        r#"{{"@type":"sticker","id":"{file}","set_id":"0","width":512,"height":512,"emoji":"{emoji}","format":{{"@type":"{format}"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":{premium}}},"thumbnail":null,"sticker":{}}}"#,
        file_json(file, name)
    )
}

/// The `messageEffect` TDLib describes `id` with.
fn effect_json(id: i64, emoji: &str, premium: bool, sticker: bool) -> String {
    let icon = sticker_json(ICON_FILE, emoji, "stickerFormatWebp", None);
    let kind = if sticker {
        format!(
            r#"{{"@type":"messageEffectTypePremiumSticker","sticker":{}}}"#,
            sticker_json(
                ANIMATION_FILE,
                emoji,
                "stickerFormatTgs",
                Some(ANIMATION_FILE + 1)
            )
        )
    } else {
        let animation = sticker_json(ANIMATION_FILE, emoji, "stickerFormatTgs", None);
        format!(
            r#"{{"@type":"messageEffectTypeEmojiReaction","select_animation":{animation},"effect_animation":{animation}}}"#
        )
    };
    format!(
        r#"{{"@type":"messageEffect","id":"{id}","emoji":"{emoji}","is_premium":{premium},"static_icon":{icon},"type":{kind}}}"#
    )
}

/// The ready chats, chat A open with two messages sent with effects and
/// every effect of the account described.
pub(super) fn seed_message_effects_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    let seq = AtomicU64::new(session.last_seq);
    let mut apply = |json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    let ids = |list: &[(i64, &str, bool)]| {
        list.iter()
            .map(|(id, _, _)| format!(r#""{id}""#))
            .collect::<Vec<_>>()
            .join(",")
    };
    apply(&format!(
        r#"{{"@type":"updateAvailableMessageEffects","reaction_effect_ids":[{}],"sticker_effect_ids":[{}]}}"#,
        ids(&REACTION_EFFECTS),
        ids(&STICKER_EFFECTS)
    ));
    for (id, emoji, premium) in REACTION_EFFECTS {
        apply(&effect_json(id, emoji, premium, false));
    }
    for (id, emoji, premium) in STICKER_EFFECTS {
        apply(&effect_json(id, emoji, premium, true));
    }
    let (fire, _, _) = REACTION_EFFECTS[0];
    let (party, _, _) = REACTION_EFFECTS[3];
    for (id, outgoing, effect, text) in [
        (98, false, party, "Happy birthday!"),
        (99, true, fire, "Thank you, this is great"),
    ] {
        apply(&format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"is_outgoing":{outgoing},"date":1790632200,"effect_id":"{effect}","content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#
        ));
    }
    session.open_chat(ChatId(CHAT));
    session
}

/// The message that arrives sent with an effect, unread, in the open chat.
pub(super) const ARRIVING: i64 = 130;

/// An incoming message sent with the party effect arrives in the open chat
/// (`QUILL_DEMO_EFFECTS=arrive`).
pub(super) fn arrive(session: &mut Session) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(session.last_seq);
    let (party, _, _) = REACTION_EFFECTS[3];
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{ARRIVING},"chat_id":{CHAT},"is_outgoing":false,"date":1790632260,"effect_id":"{party}","content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"See you at the party","entities":[]}}}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    fn demo_message_effects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("See you tonight", window, cx);
        });
        let stage = std::env::var("QUILL_DEMO_EFFECTS").unwrap_or_default();
        if stage == "window" {
            self.open_chat_in_new_window(ChatId(CHAT), window, cx);
            QuillApp::spawn_main(cx, async move |app, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1800))
                    .await;
                let _ = app.update(cx, |app, cx| {
                    if let Some(session) = app.demo_session.as_deref_mut() {
                        arrive(session);
                    }
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        if stage == "arrive" {
            if let Some(session) = self.demo_session.as_deref_mut() {
                arrive(session);
            }
            return;
        }
        if stage.is_empty() || stage == "badge" {
            return;
        }
        // Where a right-click on Send lands.
        let viewport = window.viewport_size();
        let at = point(viewport.width - px(32.), viewport.height - px(32.));
        self.open_send_menu(at, true, window, cx);
        match stage.as_str() {
            "expanded" => self.expand_send_effects(window, cx),
            "card" => self.choose_send_effect(REACTION_EFFECTS[0].0, cx),
            "premium" => self.choose_send_effect(REACTION_EFFECTS[6].0, cx),
            _ => {}
        }
    }
}
