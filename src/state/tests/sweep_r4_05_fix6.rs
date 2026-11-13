//! Fix round 6 of sweep bundle r4-05: a change note waits for every request
//! it was given, each answer matched by its `RequestId`.
use super::common::*;
use super::*;
use crate::state::fetch::ChangeNote;

fn error_json(extra: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"BROKEN"}}"#,
        extra.0,
    )
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
fn two_mask_changes_out_the_first_refused_shows_a_line() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let a = mask_change(&mut session, 11);
    let b = mask_change(&mut session, 12);
    apply_json(&mut session, &seq, &sink, &error_json(a));
    assert!(session.stickers.masks.change_note.line().is_some());
    // The screen is left: the second one's refusal lands nowhere.
    session.stickers.masks.set_dialog_open(false);
    apply_json(&mut session, &seq, &sink, &error_json(b));
    assert!(session.stickers.masks.change_note.line().is_none());
}

#[test]
fn two_unblocks_out_the_first_refused_shows_a_line() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let mut ids = Vec::new();
    for user_id in [61, 62] {
        let id = session.request(
            RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList {
                user_id,
                block: false,
            }),
            None,
        );
        session.settings.block_note.begin(id);
        ids.push(id);
    }
    apply_json(&mut session, &seq, &sink, &error_json(ids[0]));
    assert!(session.settings.block_note.line().is_some());
    session.settings.leave_blocked_list();
    apply_json(&mut session, &seq, &sink, &error_json(ids[1]));
    assert!(session.settings.block_note.line().is_none());
}

#[test]
fn a_refusal_of_a_request_never_given_lands_nowhere() {
    let mut note = ChangeNote::default();
    note.begin(RequestId(1));
    note.refused(Some(RequestId(2)), "no");
    assert!(note.line().is_none());
    note.refused(Some(RequestId(1)), "yes");
    assert_eq!(note.line(), Some("yes"));
}
