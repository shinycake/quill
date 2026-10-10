use super::*;

// Phase 4.2: `messagePoll` (schema 1.8.67 line 5241), `poll` (line 711),
// `pollOption` (line 456), `pollTypeRegular` (line 468).
fn message_poll_json(closed: bool) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":106,"chat_id":15,"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":9001,"question":{{"@type":"formattedText","text":"Lunch?","entities":[]}},"options":[{{"@type":"pollOption","id":"a","text":{{"@type":"formattedText","text":"Sushi","entities":[]}},"voter_count":12,"vote_percentage":55,"is_chosen":true}},{{"@type":"pollOption","id":"b","text":{{"@type":"formattedText","text":"Pizza","entities":[]}},"voter_count":7,"vote_percentage":32,"is_chosen":false}}],"total_voter_count":19,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":{closed},"type":{{"@type":"pollTypeRegular"}}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#
    )
}

#[test]
fn message_poll_parses_regular_open_with_chosen_option() {
    let env = parse_envelope(&message_poll_json(false)).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            let MessageContent::Poll(poll_content) = &message.content else {
                panic!("{:?}", message.content);
            };
            let poll = &poll_content.poll;
            assert_eq!(poll.id, 9001);
            assert_eq!(poll.question, "Lunch?");
            assert_eq!(poll.options.len(), 2);
            assert_eq!(poll.options[0].text, "Sushi");
            assert_eq!(poll.options[0].voter_count, 12);
            assert_eq!(poll.options[0].vote_percentage, 55);
            assert!(poll.options[0].is_chosen);
            assert!(!poll.options[1].is_chosen);
            assert_eq!(poll.total_voter_count, 19);
            assert!(poll.is_anonymous);
            assert!(!poll.allows_multiple_answers);
            assert!(poll.allows_revoting);
            assert!(!poll.is_closed);
            assert!(matches!(poll.poll_type, PollType::Regular));
            assert!(poll.can_vote());
            assert_eq!(message.content.preview(), "Lunch?");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn message_poll_parses_quiz_closed() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":107,"chat_id":15,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9002,"question":{"@type":"formattedText","text":"Red planet?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Mars","entities":[]},"voter_count":18,"vote_percentage":72,"is_chosen":false}],"total_voter_count":25,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":false,"is_closed":true,"type":{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"","entities":[]}}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            let MessageContent::Poll(poll_content) = &message.content else {
                panic!("{:?}", message.content);
            };
            let poll = &poll_content.poll;
            assert!(poll.is_closed);
            assert!(!poll.can_vote());
            match &poll.poll_type {
                PollType::Quiz {
                    correct_option_ids,
                    explanation,
                } => {
                    assert_eq!(correct_option_ids, &[0]);
                    assert!(explanation.is_empty());
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_poll_parses_and_applies_new_counts() {
    let env = parse_envelope(
            r#"{"@type":"updatePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":13,"vote_percentage":56,"is_chosen":true}],"total_voter_count":23,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdatePoll { poll }) => {
            assert_eq!(poll.id, 9001);
            assert_eq!(poll.total_voter_count, 23);
            assert_eq!(poll.options[0].voter_count, 13);
            assert_eq!(poll.options[0].vote_percentage, 56);
            assert!(poll.options[0].is_chosen);
        }
        other => panic!("{other:?}"),
    }
}
