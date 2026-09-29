//! Force-reply keyboard bar target selection
//! (`parity:bots-force-reply-keyboard`).
//!
//! Pure state logic: which message's `replyMarkupForceReply` the bar above
//! the composer renders for. Mirrors `active_custom_keyboard` (state.rs),
//! which covers `replyMarkupShowKeyboard`. The UI only renders what this
//! returns; dismissal reuses the custom-keyboard `dismissed_keyboards` set.

use std::collections::{BTreeMap, HashSet};

use crate::ids::{ChatId, MessageId};
use crate::state::HistoryMessage;
use crate::telegram::envelope::ReplyMarkup;

/// The active force-reply bar target: the latest message carrying a
/// standalone `replyMarkupForceReply`, unless dismissed. The returned
/// string is the message's `input_field_placeholder`, shown as the bar
/// label. Other markups (show/inline/remove keyboard) never produce a
/// target. Pure logic: unit-tested.
pub fn active_force_reply(
    messages: &BTreeMap<i64, HistoryMessage>,
    dismissed: &HashSet<(i64, i64)>,
) -> Option<(ChatId, MessageId, String)> {
    let mut active: Option<(ChatId, MessageId, String)> = None;
    for message in messages.values() {
        if let Some(ReplyMarkup::ForceReply { placeholder }) = &message.reply_markup {
            active = Some((message.chat_id, message.id, placeholder.clone()));
        }
    }
    active.filter(|(chat_id, message_id, _)| !dismissed.contains(&(chat_id.0, message_id.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::AccountKey;
    use crate::state::Session;
    use crate::telegram::client::copy_and_parse;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    fn session() -> (Session, Arc<MemorySink>) {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        (Session::new(AccountKey::primary(), dyn_sink), sink)
    }

    fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let owned = copy_and_parse(json, seq, &dyn_sink).expect("fixture must parse");
        session.apply(owned);
    }

    fn message_json(id: i64, markup: &str) -> String {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":21,"is_outgoing":false,"reply_markup":{markup},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}}}"#
        )
    }

    fn force_reply_json(id: i64, placeholder: &str) -> String {
        message_json(
            id,
            &format!(
                r#"{{"@type":"replyMarkupForceReply","input_field_placeholder":"{placeholder}"}}"#
            ),
        )
    }

    fn show_keyboard_json(id: i64) -> String {
        message_json(
            id,
            r#"{"@type":"replyMarkupShowKeyboard","rows":[[{"@type":"keyboardButton","text":"Yes","type":{"@type":"keyboardButtonTypeText"}}]],"is_persistent":false,"resize_keyboard":false,"one_time":true,"is_personal":false,"force_reply":false,"input_field_placeholder":""}"#,
        )
    }

    fn plain_json(id: i64) -> String {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":21,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}}}"#
        )
    }

    fn messages(session: &Session) -> &BTreeMap<i64, HistoryMessage> {
        &session.histories.get(&21).expect("history").messages
    }

    #[test]
    fn newest_force_reply_wins_and_reports_placeholder() {
        // Newest standalone `replyMarkupForceReply` wins; the bar label
        // carries its `input_field_placeholder`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &force_reply_json(306, "Type your name"),
        );
        apply_json(&mut session, &seq, &sink, &force_reply_json(307, ""));
        let none: HashSet<(i64, i64)> = HashSet::new();
        let active = active_force_reply(messages(&session), &none).expect("force-reply");
        assert_eq!(active.0, ChatId(21));
        assert_eq!(active.1, MessageId(307));
        assert_eq!(active.2, "");
    }

    #[test]
    fn dismissed_force_reply_stays_hidden() {
        // A dismissed force-reply never renders the bar again.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        apply_json(&mut session, &seq, &sink, &force_reply_json(306, "hi"));
        let dismissed: HashSet<(i64, i64)> = [(21, 306)].into_iter().collect();
        assert!(active_force_reply(messages(&session), &dismissed).is_none());
    }

    #[test]
    fn other_markups_never_produce_a_target() {
        // Show-keyboard, plain, and remove-keyboard messages are not
        // force-replies: the bar stays hidden for them.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        apply_json(&mut session, &seq, &sink, &show_keyboard_json(401));
        apply_json(&mut session, &seq, &sink, &plain_json(402));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &message_json(
                403,
                r#"{"@type":"replyMarkupRemoveKeyboard","is_personal":false}"#,
            ),
        );
        let none: HashSet<(i64, i64)> = HashSet::new();
        assert!(active_force_reply(messages(&session), &none).is_none());
    }
}
