//! Fix round 7 of sweep bundle r4-05: a change note lets each request out
//! on its ok answer, so `Refused { waiting }` holds only requests still out.
use super::common::*;
use super::*;
use crate::state::fetch::ChangeNote;

fn ok_json(extra: RequestId) -> String {
    format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0)
}

fn error_json(extra: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"BROKEN"}}"#,
        extra.0,
    )
}

fn block_change(session: &mut Session, user_id: i64) -> RequestId {
    let id = session.request(
        RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList {
            user_id,
            block: false,
        }),
        None,
    );
    session.settings.block_note.begin(id);
    id
}

fn mask_change(session: &mut Session, set_id: i64) -> RequestId {
    let id = session.request(
        StickersPurpose::ChangeMaskSet {
            set_id,
            installed: false,
            archived: true,
        }
        .into(),
        None,
    );
    session
        .stickers
        .masks
        .begin_change(id, MaskChangeTarget::Set(set_id));
    id
}

#[test]
fn an_ok_block_answer_leaves_the_note_idle() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let id = block_change(&mut session, 61);
    apply_json(&mut session, &seq, &sink, &ok_json(id));
    assert_eq!(session.settings.block_note, ChangeNote::Idle);
}

#[test]
fn an_ok_mask_answer_leaves_the_note_idle() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let id = mask_change(&mut session, 11);
    apply_json(&mut session, &seq, &sink, &ok_json(id));
    assert_eq!(session.stickers.masks.change_note, ChangeNote::Idle);
    assert!(!session.stickers.masks.change_pending());
}

#[test]
fn an_ok_session_answer_leaves_the_note_idle() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let id = session.request(
        RequestPurpose::Settings(SettingsPurpose::TerminateSession { session_id: 22 }),
        None,
    );
    session.settings.sessions_op.begin(id);
    session.settings.sessions_note.begin(id);
    apply_json(&mut session, &seq, &sink, &ok_json(id));
    assert_eq!(session.settings.sessions_note, ChangeNote::Idle);
}

#[test]
fn ok_on_the_first_block_and_a_refusal_of_the_second_waits_for_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = block_change(&mut session, 61);
    let second = block_change(&mut session, 62);
    apply_json(&mut session, &seq, &sink, &ok_json(first));
    apply_json(&mut session, &seq, &sink, &error_json(second));
    assert!(session.settings.block_note.line().is_some());
    assert!(matches!(
        &session.settings.block_note,
        ChangeNote::Refused { waiting, .. } if waiting.is_empty()
    ));
}

#[test]
fn ok_on_the_first_mask_change_and_a_refusal_of_the_second_waits_for_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = mask_change(&mut session, 11);
    let second = mask_change(&mut session, 12);
    apply_json(&mut session, &seq, &sink, &ok_json(first));
    apply_json(&mut session, &seq, &sink, &error_json(second));
    assert!(session.stickers.masks.change_note.line().is_some());
    assert!(matches!(
        &session.stickers.masks.change_note,
        ChangeNote::Refused { waiting, .. } if waiting.is_empty()
    ));
}

#[test]
fn a_settled_request_leaves_a_shown_refusal_standing() {
    let mut note = ChangeNote::default();
    note.begin(RequestId(1));
    note.begin(RequestId(2));
    note.refused(Some(RequestId(1)), "no");
    note.settled(RequestId(2));
    assert_eq!(note.line(), Some("no"));
}
