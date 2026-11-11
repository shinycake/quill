//! A refused reaction takes back only its own change (05-191).
use super::common::*;
use super::*;
use crate::telegram::envelope::ReactionType;

const CHAT: i64 = 31;
const MESSAGE: i64 = 10;

fn seeded() -> Session {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0}}"#,
        r#"{"@type":"updateNewMessage","message":{"id":10,"chat_id":31,"sender_id":{"@type":"messageSenderUser","user_id":31},"is_outgoing":false,"date":1700000000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    ] {
        apply_json(&mut session, &seq, &sink, json);
    }
    session
}

fn emoji(text: &str) -> ReactionType {
    ReactionType::Emoji { emoji: text.into() }
}

fn own(session: &Session) -> Vec<String> {
    session.histories[&CHAT].messages[&MESSAGE]
        .interaction_info
        .as_ref()
        .and_then(|info| info.reactions.as_ref())
        .map(|reactions| {
            reactions
                .reactions
                .iter()
                .filter(|one| one.is_chosen)
                .filter_map(|one| one.reaction_type.emoji_text().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn react(session: &mut Session, extra: u64, text: &str) -> RequestId {
    let extra = RequestId(extra);
    session.set_own_reaction_for(extra, ChatId(CHAT), MessageId(MESSAGE), &emoji(text), true);
    extra
}

#[test]
fn a_refused_first_reaction_keeps_the_one_stacked_on_it() {
    let mut session = seeded();
    let a = react(&mut session, 1, "👍");
    let _b = react(&mut session, 2, "🔥");
    assert_eq!(own(&session), ["👍", "🔥"]);
    session.settle_own_reaction(Some(a), true);
    assert_eq!(own(&session), ["🔥"]);
}

#[test]
fn a_refused_lone_reaction_restores_what_was_there() {
    let mut session = seeded();
    let a = react(&mut session, 1, "👍");
    session.settle_own_reaction(Some(a), true);
    assert!(own(&session).is_empty());
    let before = session.histories[&CHAT].messages[&MESSAGE]
        .interaction_info
        .clone();
    assert_eq!(before, None);
}

#[test]
fn an_accepted_reaction_stays() {
    let mut session = seeded();
    let a = react(&mut session, 1, "👍");
    session.settle_own_reaction(Some(a), false);
    assert_eq!(own(&session), ["👍"]);
    assert!(session.messages.reaction_undo.is_empty());
}
