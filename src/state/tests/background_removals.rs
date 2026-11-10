//! Wallpaper removals apply on the server's answer, and the preview's
//! answers are matched to the request that asked (sweep 02-087, 02-089).
use super::common::*;
use super::*;

const SKY: &str = r#"{"@type":"background","id":5,"is_default":false,"is_dark":false,"name":"sky","type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillSolid","color":2}}}"#;

fn with_list() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetInstalledBackgrounds, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"backgrounds","@extra":"{}","backgrounds":[{SKY}]}}"#,
            extra.0
        ),
    );
    (session, sink, seq)
}

fn answer(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    extra: RequestId,
    ok: bool,
) {
    let body = if ok {
        r#""@type":"ok""#
    } else {
        r#""@type":"error","code":400,"message":"BACKGROUND_INVALID""#
    };
    apply_json(
        session,
        seq,
        sink,
        &format!(r#"{{{body},"@extra":"{}"}}"#, extra.0),
    );
}

#[test]
fn a_refused_removal_keeps_the_installed_wallpaper() {
    let (mut session, sink, seq) = with_list();
    let id = session.request(
        RequestPurpose::Chats(ChatsPurpose::RemoveInstalledBackground { background_id: 5 }),
        None,
    );
    assert_eq!(
        session
            .chats_state
            .installed_backgrounds
            .data()
            .map(Vec::len),
        Some(1)
    );
    answer(&mut session, &seq, &sink, id, false);
    assert_eq!(
        session
            .chats_state
            .installed_backgrounds
            .data()
            .map(Vec::len),
        Some(1)
    );
    assert!(session.chats_state.background_error.is_some());
    let id = session.request(
        RequestPurpose::Chats(ChatsPurpose::RemoveInstalledBackground { background_id: 5 }),
        None,
    );
    answer(&mut session, &seq, &sink, id, true);
    assert_eq!(
        session
            .chats_state
            .installed_backgrounds
            .data()
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn a_refused_default_and_chat_delete_leave_the_wallpaper() {
    let (mut session, sink, seq) = with_list();
    session.chats_state.default_backgrounds.insert(
        true,
        session
            .chats_state
            .installed_backgrounds
            .data()
            .cloned()
            .unwrap_or_default()[0]
            .clone(),
    );
    let id = session.request(
        RequestPurpose::Chats(ChatsPurpose::DeleteDefaultBackground {
            for_dark_theme: true,
        }),
        None,
    );
    answer(&mut session, &seq, &sink, id, false);
    assert!(session.chats_state.default_backgrounds.contains_key(&true));
    let id = session.request(
        RequestPurpose::Chats(ChatsPurpose::DeleteDefaultBackground {
            for_dark_theme: true,
        }),
        None,
    );
    answer(&mut session, &seq, &sink, id, true);
    assert!(!session.chats_state.default_backgrounds.contains_key(&true));
}

#[test]
fn preview_answers_are_taken_by_the_request_that_asked() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = session.request(RequestPurpose::GetBackgroundUrl, None);
    let second = session.request(RequestPurpose::SetDefaultBackground, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"httpUrl","@extra":"{}","url":"https://t.me/bg/a"}}"#,
            first.0
        ),
    );
    answer(&mut session, &seq, &sink, second, false);
    assert!(
        session
            .chats_state
            .background_answers
            .take_for(&[RequestId(u64::MAX)])
            .is_empty()
    );
    assert!(
        session.chats_state.background_answers.is_empty(),
        "an answer nobody waits for is dropped"
    );
    let third = session.request(RequestPurpose::SetDefaultBackground, None);
    answer(&mut session, &seq, &sink, third, false);
    assert!(matches!(
        session
            .chats_state
            .background_answers
            .take_for(&[third])
            .as_slice(),
        [(_, Err(_))]
    ));
}

#[test]
fn appearance_requests_answer_under_their_own_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let name = session.request(RequestPurpose::SetChatAccentColor, Some(ChatId(7)));
    let pack = session.request(
        RequestPurpose::SetSupergroupCustomEmojiStickerSet,
        Some(ChatId(7)),
    );
    answer(&mut session, &seq, &sink, name, true);
    answer(&mut session, &seq, &sink, pack, false);
    let both = session
        .chats_state
        .appearance_answers
        .take_for(&[name, pack]);
    assert!(matches!(both.as_slice(), [(_, Ok(())), (_, Err(_))]));
    answer(&mut session, &seq, &sink, pack, false);
    assert!(
        session
            .chats_state
            .appearance_answers
            .take_for(&[name])
            .is_empty()
    );
}
