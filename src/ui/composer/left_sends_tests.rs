//! A rich send whose chat view was replaced still waits for its answer.
use crate::ui::composer::LeftSend;
use crate::ui::composer_sends::{ComposerSends, SendSlot};
use quill::composer::DraftTopic;
use quill::ids::{AccountKey, RequestId};
use quill::state::Answer;

fn send(request: u64) -> LeftSend {
    LeftSend {
        request: RequestId(request),
        slot: SendSlot {
            account: AccountKey::primary(),
            draft: DraftTopic::Chat.key(7),
        },
        markup: format!("text {request}"),
        rich: true,
    }
}

#[test]
fn a_left_send_leaves_with_its_own_answer_only() {
    let mut sends = ComposerSends::default();
    assert!(sends.keep_left_send(send(1)).is_none());
    assert!(sends.keep_left_send(send(2)).is_none());
    let answered = sends.take_left_sends(&AccountKey::primary(), |request| {
        (request == RequestId(2)).then(|| Answer::Failed("FLOOD".into()))
    });
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0].0.markup, "text 2", "its text comes back");
    // The other still waits; its answer settles it later.
    let answered = sends.take_left_sends(&AccountKey::primary(), |_| Some(Answer::Done));
    assert_eq!(answered.len(), 1);
    assert!(
        sends
            .take_left_sends(&AccountKey::primary(), |_| Some(Answer::Done))
            .is_empty(),
        "nothing outlives its answer"
    );
}

#[test]
fn too_many_left_sends_forget_the_oldest() {
    let mut sends = ComposerSends::default();
    let mut dropped = None;
    for request in 1..=9 {
        dropped = sends.keep_left_send(send(request)).or(dropped);
    }
    assert_eq!(dropped.map(|old| old.request), Some(RequestId(1)));
}
