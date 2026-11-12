//! A scheduled delete leaves with its own answer; a refusal asks for the
//! chat's list again.
use super::{ScheduledDelete, take_refused};
use quill::ids::{ChatId, RequestId};
use quill::state::Answer;

fn delete(request: u64, chat: i64) -> ScheduledDelete {
    ScheduledDelete {
        request: RequestId(request),
        chat_id: ChatId(chat),
    }
}

#[test]
fn a_refused_delete_asks_for_its_chat_list_and_leaves() {
    let mut pending = vec![delete(1, 10), delete(2, 20), delete(3, 30)];
    let refused = take_refused(&mut pending, |request| match request.0 {
        1 => Some(Answer::Failed("MESSAGE_ID_INVALID".into())),
        2 => Some(Answer::Done),
        _ => None,
    });
    assert_eq!(refused, [ChatId(10)], "only the refused chat reloads");
    assert_eq!(pending, [delete(3, 30)], "the unanswered one still waits");
}

#[test]
fn answers_in_either_order_settle_each_delete_once() {
    let mut pending = vec![delete(1, 10), delete(2, 10)];
    let refused = take_refused(&mut pending, |request| {
        (request.0 == 2).then(|| Answer::Failed("offline".into()))
    });
    assert_eq!(refused, [ChatId(10)]);
    let refused = take_refused(&mut pending, |_| Some(Answer::Done));
    assert!(refused.is_empty(), "the later success clears nothing else");
    assert!(pending.is_empty());
}
