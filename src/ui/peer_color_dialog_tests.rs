#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The color box's Apply waits for the answers of its own requests
//! (sweep 11-044, 11-045): it is `Sent` only when every request was
//! accepted, `Failed` with the refusal when one was not, and another
//! request's answer never lands in it.

use crate::ui::peer_color_dialog::ApplyStage;
use quill::ids::RequestId;
use quill::state::{Answer, RequestAcks};

fn applying(ids: &[u64]) -> ApplyStage {
    ApplyStage::Applying {
        waiting: ids.iter().map(|id| RequestId(*id)).collect(),
        refused: None,
    }
}

fn answered(id: u64, answer: Answer) -> RequestAcks {
    let mut acks = RequestAcks::default();
    acks.expect(RequestId(id));
    acks.settle(RequestId(id), answer);
    acks
}

#[test]
fn applying_ends_sent_only_when_every_request_was_accepted() {
    let mut stage = applying(&[1, 2]);
    assert!(stage.settle(&mut answered(1, Answer::Done)));
    assert!(
        matches!(stage, ApplyStage::Applying { .. }),
        "2 still waits"
    );
    assert!(stage.settle(&mut answered(2, Answer::Done)));
    assert_eq!(stage, ApplyStage::Sent);
}

#[test]
fn a_refusal_ends_failed_in_either_order() {
    let mut stage = applying(&[1, 2]);
    stage.settle(&mut answered(2, Answer::Failed("PREMIUM".into())));
    assert!(matches!(stage, ApplyStage::Applying { .. }));
    stage.settle(&mut answered(1, Answer::Done));
    assert!(matches!(&stage, ApplyStage::Failed(why) if why.contains("PREMIUM")));

    let mut stage = applying(&[1, 2]);
    stage.settle(&mut answered(1, Answer::Done));
    stage.settle(&mut answered(2, Answer::Failed("PREMIUM".into())));
    assert!(matches!(stage, ApplyStage::Failed(_)));
}

#[test]
fn another_requests_answer_changes_nothing() {
    let mut stage = applying(&[1]);
    assert!(!stage.settle(&mut answered(9, Answer::Failed("X".into()))));
    assert_eq!(stage, applying(&[1]));
    let mut editing = ApplyStage::Editing;
    assert!(!editing.settle(&mut answered(1, Answer::Done)));
}
