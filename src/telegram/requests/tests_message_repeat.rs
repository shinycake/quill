use crate::composer::{ComposerScheduling, SendOptions};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::requests::*;
use serde_json::Value;

#[test]
fn a_repeat_period_rides_a_scheduled_send_only() {
    let repeating = SendOptions {
        scheduling: ComposerScheduling::SendAtDate(1_700_000_000),
        schedule_repeat_period: 7 * 86_400,
        ..SendOptions::default()
    };
    let v: Value = serde_json::from_str(&send_text(
        RequestId(1),
        ChatId(11),
        None,
        "hi",
        None,
        &repeating,
    ))
    .unwrap();
    assert_eq!(
        v["options"]["scheduling_state"]["repeat_period"],
        7 * 86_400
    );

    // Without a send date there is nothing to repeat.
    let loose = SendOptions {
        schedule_repeat_period: 86_400,
        disable_notification: true,
        ..SendOptions::default()
    };
    let v: Value = serde_json::from_str(&send_text(
        RequestId(2),
        ChatId(11),
        None,
        "hi",
        None,
        &loose,
    ))
    .unwrap();
    assert!(v["options"]["scheduling_state"].is_null());
}

#[test]
fn forwarding_repeats_only_a_single_message() {
    let repeating = SendOptions {
        scheduling: ComposerScheduling::SendAtDate(1_700_000_000),
        schedule_repeat_period: 86_400,
        ..SendOptions::default()
    };
    let forward = |ids: &[MessageId]| -> Value {
        serde_json::from_str(&forward_messages_with_options(
            RequestId(3),
            ChatId(11),
            ChatId(12),
            ids,
            false,
            false,
            &repeating,
        ))
        .unwrap()
    };
    let one = forward(&[MessageId(40)]);
    assert_eq!(one["options"]["scheduling_state"]["repeat_period"], 86_400);
    let two = forward(&[MessageId(40), MessageId(41)]);
    assert_eq!(two["options"]["scheduling_state"]["repeat_period"], 0);
    assert_eq!(
        two["options"]["scheduling_state"]["send_date"],
        1_700_000_000
    );
}
