#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The edit profile box settles a username or photo operation on its own
//! request's answer (sweep 12-B010): another request's answer never lands
//! in it, and a refusal stays until a newer operation replaces it.

use crate::ui::dialogs::edit_profile::SectionOp;
use quill::ids::RequestId;
use quill::state::{Answer, RequestAcks};

fn answered(id: u64, answer: Answer) -> RequestAcks {
    let mut acks = RequestAcks::default();
    acks.expect(RequestId(id));
    acks.settle(RequestId(id), answer);
    acks
}

#[test]
fn an_accepted_operation_leaves_no_error() {
    let mut op = SectionOp::Sending(RequestId(4));
    assert!(op.settle(&mut answered(4, Answer::Done)));
    assert_eq!(op, SectionOp::Idle);
}

#[test]
fn a_refusal_is_kept_with_its_reason() {
    let mut op = SectionOp::Sending(RequestId(4));
    assert!(op.settle(&mut answered(4, Answer::Failed("USERNAME_OCCUPIED".into()))));
    assert_eq!(op.error(), Some("USERNAME_OCCUPIED"));
    assert_eq!(op.in_flight(), None);
}

#[test]
fn another_requests_answer_changes_nothing() {
    let mut op = SectionOp::Sending(RequestId(4));
    assert!(!op.settle(&mut answered(9, Answer::Failed("X".into()))));
    assert_eq!(op, SectionOp::Sending(RequestId(4)));
    let mut idle = SectionOp::Idle;
    assert!(!idle.settle(&mut answered(4, Answer::Done)));
}
