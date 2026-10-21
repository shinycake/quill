//! Quotes chosen from rich messages (`textQuote.rich_texts`): parsed through
//! the reducer into the reply strip, and cut back into `rich_texts` for the
//! next reply.
use super::common::*;
use super::*;
use crate::composer::{ComposerReplyTo, QuoteSelection};
use crate::telegram::requests::send_reply_value;
use crate::text::TextEntityKind;

const CHAT: i64 = 11;

const RICH: &str = r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":11,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageRichMessage","message":{"@type":"richMessage","is_rtl":false,"is_full":true,"blocks":[
{"@type":"pageBlockParagraph","text":{"@type":"richTexts","texts":[
 {"@type":"richTextPlain","text":"Ship the "},
 {"@type":"richTextBold","text":{"@type":"richTextPlain","text":"quote"}},
 {"@type":"richTextPlain","text":" feature"}]}},
{"@type":"pageBlockDivider"},
{"@type":"pageBlockParagraph","text":{"@type":"richTextItalic","text":{"@type":"richTextPlain","text":"then rest"}}}
]}}}}"#;

fn seeded() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(&mut session, &seq, &sink, RICH);
    session.open_chat(ChatId(CHAT));
    (session, sink, seq)
}

fn reply_with(quote: &str) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":51,"chat_id":11,"is_outgoing":true,"date":1700000100,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"ok","entities":[]}}}},"reply_to":{{"@type":"messageReplyToMessage","chat_id":11,"message_id":50,"quote":{quote},"checklist_task_id":0,"poll_option_id":""}}}}}}"#
    )
}

#[test]
fn rich_quote_reaches_the_reply_strip_with_its_formatting() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &reply_with(
            r#"{"@type":"textQuote","text":{"@type":"formattedText","text":"","entities":[]},"rich_texts":[
{"@type":"richTexts","texts":[{"@type":"richTextBold","text":{"@type":"richTextPlain","text":"quote"}},{"@type":"richTextUrl","url":"https://x.y","is_cached":false,"text":{"@type":"richTextPlain","text":" feature"}}]},
{"@type":"richTextItalic","text":{"@type":"richTextPlain","text":"then"}}
],"position":9,"is_manual":true}"#,
        ),
    );
    let reply = session.histories[&CHAT].messages[&51].clone();
    let header = session.reply_header(&reply).expect("reply header");
    assert!(header.is_quote);
    assert_eq!(header.text, "quote feature\nthen");
    assert_eq!(
        header
            .entities
            .iter()
            .map(|entity| (entity.utf8_start, entity.utf8_end, entity.kind.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, 5, TextEntityKind::Bold),
            (14, 18, TextEntityKind::Italic),
        ]
    );
}

#[test]
fn plain_and_unknown_rich_quotes_fall_back_to_text() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &reply_with(
            r#"{"@type":"textQuote","text":{"@type":"formattedText","text":"plain words","entities":[]},"rich_texts":[],"position":0,"is_manual":true}"#,
        ),
    );
    let reply = session.histories[&CHAT].messages[&51].clone();
    let header = session.reply_header(&reply).expect("reply header");
    assert_eq!(header.text, "plain words");
    assert!(header.entities.is_empty());

    apply_json(
        &mut session,
        &seq,
        &sink,
        &reply_with(
            r#"{"@type":"textQuote","text":{"@type":"formattedText","text":"","entities":[]},"rich_texts":[
{"@type":"richTextFromTheFuture","text":{"@type":"richTextPlain","text":"odd kind"}}
],"position":0,"is_manual":true}"#,
        ),
    );
    let reply = session.histories[&CHAT].messages[&51].clone();
    let header = session.reply_header(&reply).expect("reply header");
    assert_eq!(header.text, "odd kind");
    assert!(header.entities.is_empty());
}

#[test]
fn quoting_a_rich_message_sends_rich_texts() {
    let (session, _sink, _seq) = seeded();
    let message = &session.histories[&CHAT].messages[&50];
    let content = effective_content(&message.content, message.ephemeral.as_ref());
    let full = crate::composer::rich_quote_source_text(content).expect("quotable texts");
    assert_eq!(full, "Ship the quote feature\nthen rest");

    let quote = "quote feature\nthen";
    let position = crate::composer::quote_position(&full, quote).expect("verbatim");
    let selection = QuoteSelection::in_content(content, quote.to_string(), position);
    assert_eq!(selection.rich_texts.len(), 2);

    let reply = ComposerReplyTo::with_quote(ChatId(CHAT), MessageId(50), selection);
    let target = reply.send_target(ChatId(CHAT)).expect("same chat");
    let json = send_reply_value(Some(&target));
    assert_eq!(json["quote"]["@type"], "inputTextQuote");
    assert_eq!(json["quote"]["position"], 9);
    assert_eq!(
        json["quote"]["rich_texts"],
        serde_json::json!([
            {"@type": "richTexts", "texts": [
                {"@type": "richTextBold", "text": {"@type": "richTextPlain", "text": "quote"}},
                {"@type": "richTextPlain", "text": " feature"}
            ]},
            {"@type": "richTextItalic", "text": {"@type": "richTextPlain", "text": "then"}}
        ])
    );
}

#[test]
fn a_plain_message_quote_stays_plain() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":11,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello there","entities":[]}}}}"#,
    );
    let message = &session.histories[&CHAT].messages[&60];
    let content = effective_content(&message.content, message.ephemeral.as_ref());
    assert!(crate::composer::rich_quote_source_text(content).is_none());
    let selection = QuoteSelection::in_content(content, "there".into(), 6);
    assert!(selection.rich_texts.is_empty());
    let reply = ComposerReplyTo::with_quote(ChatId(CHAT), MessageId(60), selection);
    let json = send_reply_value(reply.send_target(ChatId(CHAT)).as_ref());
    assert_eq!(json["quote"]["rich_texts"], serde_json::json!([]));
    assert_eq!(json["quote"]["text"]["text"], "there");
}
