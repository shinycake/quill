//! `ready-translate` screenshot demo: a chat in Russian (and Hebrew
//! previews) with the translate bar, translated bubbles, the translate box,
//! the language chooser and the translation settings, injected through the
//! normal reducer — no live Telegram. `QUILL_DEMO_TRANSLATE_VIEW` picks the
//! scene: `bar` (default), `translated`, `box`, `rtl`, `selection`,
//! `chooser`, `settings`, `skip`.

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{Session, TranslateJob, TranslateTarget, Translation};
use quill::telegram::client::copy_and_parse;
use quill::text::{TextEntity, TextEntityKind};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(super) const CHAT: i64 = 71;
const DANA: i64 = 72;

/// Incoming messages: original, English translation, and whether the
/// translation has a bold span `(start, end)` in bytes.
pub(super) const SCRIPT: &[(&str, &str, Option<(usize, usize)>)] = &[
    (
        "Привет! Как дела? Давно не виделись, надо бы встретиться на выходных.",
        "Hi! How are you? Haven't seen you in ages, we should meet up this weekend.",
        None,
    ),
    (
        "Я нашла отличное кафе рядом с метро, там вкусный кофе и тихо.",
        "I found a great cafe near the metro, the coffee is good and it's quiet.",
        Some((16, 20)),
    ),
    (
        "В субботу в два часа тебе подойдет? Если нет, можно в воскресенье.",
        "Does two o'clock on Saturday work for you? If not, Sunday is fine too.",
        None,
    ),
    (
        "Кстати, не забудь взять с собой ноутбук, покажу тебе новый проект.",
        "By the way, don't forget to bring your laptop, I'll show you the new project.",
        Some((39, 45)),
    ),
    (
        "Мы вчера закончили первую версию, и она работает намного быстрее, чем раньше.",
        "We finished the first version yesterday, and it works much faster than before.",
        None,
    ),
    (
        "Еще нужно проверить несколько мелочей, но в целом все выглядит хорошо.",
        "A few small things still need checking, but overall everything looks good.",
        None,
    ),
    (
        "Напиши, когда будешь выходить из дома, я встречу тебя у входа.",
        "Text me when you leave home, I'll meet you at the entrance.",
        None,
    ),
    (
        "Спасибо большое за помощь с документами, ты меня очень выручила.",
        "Thank you so much for helping with the documents, you really saved me.",
        None,
    ),
    (
        "Погода обещает быть солнечной, так что можно погулять в парке после кафе.",
        "The weather should be sunny, so we can walk in the park after the cafe.",
        None,
    ),
    (
        "Жду с нетерпением, до встречи!",
        "Looking forward to it, see you soon!",
        None,
    ),
];

fn apply(session: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64, json: &str) {
    if let Some(owned) = copy_and_parse(json, seq, sink) {
        session.apply(owned);
    }
}

fn entities(spans: Option<(usize, usize)>) -> Vec<TextEntity> {
    spans
        .map(|(start, end)| TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind: TextEntityKind::Bold,
        })
        .into_iter()
        .collect()
}

/// Inject the chat, a Premium account and TDLib's translate flag; the
/// scene decides what is on screen.
pub(super) fn apply_ready_translate(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 3600;
    apply(
        session,
        &dyn_sink,
        seq,
        &format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{DANA},"first_name":"Dana","last_name":"Weiss","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"accent_color_id":2,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
    );
    apply(
        session,
        &dyn_sink,
        seq,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Dana Weiss","type":{{"@type":"chatTypePrivate","user_id":{DANA}}},"unread_count":0,"is_translatable":true}}}}"#
        ),
    );
    apply(
        session,
        &dyn_sink,
        seq,
        &format!(
            r#"{{"@type":"updateChatPosition","chat_id":{CHAT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"4000","is_pinned":false}}}}"#
        ),
    );
    for (index, (text, _, _)) in SCRIPT.iter().enumerate() {
        let text = serde_json::to_string(text).unwrap_or_default();
        apply(
            session,
            &dyn_sink,
            seq,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"sender_id":{{"@type":"messageSenderUser","user_id":{DANA}}},"is_outgoing":false,"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}}}}}"#,
                id = 1000 + index as i64,
                date = now + index as i64 * 90,
            ),
        );
    }
    session.premium_option = Some(true);
    session.open_chat(ChatId(CHAT));

    let done = |index: usize| {
        let (_, english, bold) = SCRIPT[index];
        Translation::Done {
            text: english.to_string(),
            entities: entities(bold),
        }
    };
    if matches!(view, "translated" | "box" | "selection") {
        for index in 0..SCRIPT.len() {
            session
                .translate
                .messages
                .insert((CHAT, 1000 + index as i64, "en".to_string()), done(index));
        }
    }
    if view == "translated" {
        session.set_chat_translated_to(ChatId(CHAT), Some("en"));
    }
    if view == "rtl" {
        session.translate.messages.insert(
            (CHAT, 1001, "he".to_string()),
            Translation::Done {
                text: "מצאתי בית קפה מעולה ליד התחנה, הקפה טעים והמקום שקט. Telegram אומר שזה פתוח עד חצות.".to_string(),
                entities: entities(Some((0, 11))),
            },
        );
    }
    // Keep the unused-import guard honest: a job table entry for a pending
    // selection translation is what a live request would leave behind.
    if view == "selection" {
        session.translate.jobs.insert(
            1,
            TranslateJob {
                target: TranslateTarget::Text,
                to_language: "en".into(),
            },
        );
        session.translate.texts.insert(
            1,
            Translation::Done {
                text: "I found a great cafe near the metro".to_string(),
                entities: Vec::new(),
            },
        );
    }
    let _ = MessageId(0);
}
