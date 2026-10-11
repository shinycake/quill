//! State reducer tests: animated emoji taps.
use super::common::*;
use super::*;

const STICKER: &str = r#"{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"👍","format":{"@type":"stickerFormatTgs"},"full_type":{"@type":"stickerFullTypeRegular","premium_animation":null},"thumbnail":null,"sticker":{"@type":"file","id":42,"size":7,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false},"remote":{"@type":"remoteFile","id":"r","unique_id":"u"}}}"#;

fn apply(session: &mut Session, json: &str) {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let sink = Arc::new(MemorySink::new());
    apply_json(session, &SEQ, &sink, json);
}

fn update(chat: i64) -> String {
    format!(
        r#"{{"@type":"updateAnimatedEmojiMessageClicked","chat_id":{chat},"message_id":8388608,"sticker":{STICKER}}}"#
    )
}

#[test]
fn incoming_click_queues_an_effect_in_the_open_chat() {
    let (mut session, _sink) = session();
    session.open_chat = Some(ChatId(5));
    apply(&mut session, &update(5));
    let taken = session.stickers.interactions.take_pending();
    assert_eq!(taken.len(), 1);
    assert!(taken[0].incoming);
    assert_eq!(taken[0].message_id, MessageId(8_388_608));
    assert!(session.media.files.contains_key(&42));
}

#[test]
fn a_click_in_another_chat_plays_nothing() {
    let (mut session, _sink) = session();
    session.open_chat = Some(ChatId(6));
    apply(&mut session, &update(5));
    assert!(session.stickers.interactions.take_pending().is_empty());
}
