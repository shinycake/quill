//! Rapid taps on a multiple-answer poll build on the newest answer in
//! flight, not on marks an older answer's `updatePoll` put back.
use super::super::*;
use super::*;
use crate::ids::{ChatId, MessageId};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageContent;
use serde_json::Value;

fn multi_poll(chosen: [bool; 3]) -> String {
    let option = |id: &str, text: &str, chosen: bool| {
        format!(
            r#"{{"@type":"pollOption","id":"{id}","text":{{"@type":"formattedText","text":"{text}","entities":[]}},"voter_count":1,"vote_percentage":33,"is_chosen":{chosen}}}"#
        )
    };
    format!(
        r#"{{"@type":"poll","id":9003,"question":{{"@type":"formattedText","text":"Toppings?","entities":[]}},"options":[{},{},{}],"total_voter_count":3,"is_anonymous":true,"allows_multiple_answers":true,"allows_revoting":true,"is_closed":false,"type":{{"@type":"pollTypeRegular"}}}}"#,
        option("a", "Cheese", chosen[0]),
        option("b", "Ham", chosen[1]),
        option("c", "Olives", chosen[2]),
    )
}

fn last_answer(recorder: &RecordingSender) -> Value {
    let json = recorder.snapshot().last().cloned().expect("setPollAnswer");
    serde_json::from_str(&json).unwrap()
}

#[test]
fn a_late_update_does_not_make_the_next_tap_drop_the_first_choice() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    let message = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":108,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#,
        multi_poll([false; 3])
    );
    driver
        .ingest(copy_and_parse(&message, &seq, &dyn_sink).unwrap())
        .unwrap();

    driver.send_poll_answer(ChatId(7), MessageId(108), 0).unwrap();
    assert_eq!(last_answer(&recorder)["option_ids"], serde_json::json!([0]));
    driver.send_poll_answer(ChatId(7), MessageId(108), 1).unwrap();
    assert_eq!(last_answer(&recorder)["option_ids"], serde_json::json!([0, 1]));

    // The server's update for the first answer lands while the second is
    // still in flight: it shows only the first choice.
    let update = format!(
        r#"{{"@type":"updatePoll","poll":{}}}"#,
        multi_poll([true, false, false])
    );
    driver
        .ingest(copy_and_parse(&update, &seq, &dyn_sink).unwrap())
        .unwrap();
    let marks = |driver: &ConnectDriver<_>| -> Vec<bool> {
        match &driver
            .session
            .histories
            .get(&7)
            .and_then(|history| history.messages.get(&108))
            .map(|message| &message.content)
        {
            Some(MessageContent::Poll(poll)) => {
                poll.poll.options.iter().map(|o| o.is_chosen).collect()
            }
            _ => Vec::new(),
        }
    };
    assert_eq!(marks(&driver), vec![true, false, false]);

    driver.send_poll_answer(ChatId(7), MessageId(108), 2).unwrap();
    assert_eq!(
        last_answer(&recorder)["option_ids"],
        serde_json::json!([0, 1, 2]),
        "the first two choices stay"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
