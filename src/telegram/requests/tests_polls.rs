use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::requests::*;

#[test]
fn get_poll_voters_shape_matches_1_8_67() {
    // Verbatim constructor at schema 1.8.67 line 12941 — the pin
    // keeps the request shape honest if the schema is ever repinned.
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(schema.contains(
        "getPollVoters chat_id:int53 message_id:int53 option_id:int32 \
             offset:int32 limit:int32 = PollVoters;"
    ));
    let json = get_poll_voters(RequestId(9), ChatId(1), MessageId(42), 2, 50, 50);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getPollVoters");
    assert_eq!(v["@extra"], "9");
    assert_eq!(v["chat_id"], 1);
    assert_eq!(v["message_id"], 42);
    assert_eq!(v["option_id"], 2);
    assert_eq!(v["offset"], 50);
    assert_eq!(v["limit"], 50);
}

#[test]
fn get_inline_query_results_shape_matches_1_8_67() {
    // Verbatim constructor at schema 1.8.67 line 13019 — the pin
    // keeps the request shape honest if the schema is ever repinned.
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(schema.contains(
        "getInlineQueryResults bot_user_id:int53 chat_id:int53 \
             user_location:location query:string offset:string = InlineQueryResults;"
    ));
    let json = get_inline_query_results(RequestId(9), 77, ChatId(1), "@gif cats", "");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getInlineQueryResults");
    assert_eq!(v["@extra"], "9");
    assert_eq!(v["bot_user_id"], 77);
    assert_eq!(v["chat_id"], 1);
    // Schema doc: "pass null if unknown".
    assert!(v["user_location"].is_null());
    assert_eq!(v["query"], "@gif cats");
    // "" is the first-chunk offset (schema doc on line 13019).
    assert_eq!(v["offset"], "");
    // The pagination token passes through untouched.
    let json = get_inline_query_results(RequestId(10), 77, ChatId(1), "@gif cats", "50");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@extra"], "10");
    assert_eq!(v["offset"], "50");
}

#[test]
fn send_inline_query_result_message_shape_matches_1_8_67() {
    // Verbatim constructor at schema 1.8.67 line 12226.
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(schema.contains(
        "sendInlineQueryResultMessage chat_id:int53 topic_id:MessageTopic \
             reply_to:InputMessageReplyTo options:messageSendOptions query_id:int64 \
             result_id:string hide_via_bot:Bool = Message;"
    ));
    let json =
        send_inline_query_result_message(RequestId(9), ChatId(1), None, None, 12345, "res1", false);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendInlineQueryResultMessage");
    assert_eq!(v["@extra"], "9");
    assert_eq!(v["chat_id"], 1);
    assert!(v["topic_id"].is_null());
    assert!(v["reply_to"].is_null());
    // Null options = default message send options, like `send_document`.
    assert!(v["options"].is_null());
    assert_eq!(v["query_id"], 12345);
    assert_eq!(v["result_id"], "res1");
    assert_eq!(v["hide_via_bot"], false);
    // The forum-topic variant mirrors `send_document`.
    let json = send_inline_query_result_message(
        RequestId(9),
        ChatId(1),
        Some(7),
        None,
        12345,
        "res1",
        true,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
    assert_eq!(v["topic_id"]["forum_topic_id"], 7);
    assert_eq!(v["hide_via_bot"], true);
}

#[test]
fn stop_poll_shape_matches_1_8_67() {
    // Verbatim constructor at schema 1.8.67 line 12953.
    // `reply_markup` is "for bots only; pass null if none" (schema
    // doc) — the human client always sends null.
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(
        schema.contains("stopPoll chat_id:int53 message_id:int53 reply_markup:ReplyMarkup = Ok;")
    );
    let json = stop_poll(RequestId(9), ChatId(1), MessageId(42));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "stopPoll");
    assert_eq!(v["@extra"], "9");
    assert_eq!(v["chat_id"], 1);
    assert_eq!(v["message_id"], 42);
    assert!(v["reply_markup"].is_null());
}

#[test]
fn set_poll_answer_shape_matches_1_8_67() {
    // `setPollAnswer chat_id:int53 message_id:int53
    // option_ids:vector<int32> = Ok` (schema 1.8.67 line 12932).
    // `option_ids` are 0-based option *positions*, not `pollOption.id`
    // strings (those are `updatePollAnswer.option_ids`, line 11186).
    let json = set_poll_answer(RequestId(9), ChatId(1), MessageId(42), &[0, 2]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setPollAnswer");
    assert_eq!(v["@extra"], "9");
    assert_eq!(v["chat_id"], 1);
    assert_eq!(v["message_id"], 42);
    assert_eq!(v["option_ids"], serde_json::json!([0, 2]));
}

#[test]
fn set_poll_answer_retract_is_empty_option_ids() {
    // Retracting a vote sends an empty `option_ids` vector (still a
    // `setPollAnswer`, not a different constructor).
    let json = set_poll_answer(RequestId(9), ChatId(1), MessageId(42), &[]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setPollAnswer");
    assert_eq!(v["option_ids"], serde_json::json!([]));
}

#[test]
fn send_poll_shape_matches_1_8_67() {
    // `inputMessagePoll` (schema 1.8.67 line 6193) wrapped in
    // `sendMessage`; options are `inputPollOption` (line 462) and the
    // type is `inputPollTypeRegular` (line 481).
    let options = ["Sushi place", "Pizza"];
    let countries = ["US", "GB"];
    let json = send_poll(
        RequestId(11),
        ChatId(7),
        PollSend {
            question: "Where should we eat lunch?",
            options: &options,
            description: "team lunch",
            is_anonymous: true,
            allows_multiple_answers: false,
            allows_revoting: false,
            shuffle_options: true,
            country_codes: &countries,
            poll_type: PollTypeSend::Regular,
            open_period: 3 * 3600,
            allow_adding_options: true,
            hide_results_until_closes: true,
            members_only: true,
            close_date: 0,
            reply_to: Some(SendReply::plain(MessageId(101))),
            topic_id: None,
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], "11");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(v["reply_to"]["message_id"], 101);
    let content = &v["input_message_content"];
    assert_eq!(content["@type"], "inputMessagePoll");
    assert_eq!(content["question"]["text"], "Where should we eat lunch?");
    assert_eq!(content["question"]["entities"], serde_json::json!([]));
    assert_eq!(content["options"][0]["@type"], "inputPollOption");
    assert_eq!(content["options"][0]["text"]["text"], "Sushi place");
    assert!(content["options"][0]["media"].is_null());
    assert_eq!(content["options"][1]["text"]["text"], "Pizza");
    assert_eq!(content["description"]["@type"], "formattedText");
    assert_eq!(content["description"]["text"], "team lunch");
    assert!(content["media"].is_null());
    assert_eq!(content["is_anonymous"], true);
    assert_eq!(content["allows_multiple_answers"], false);
    assert_eq!(content["allows_revoting"], false);
    assert_eq!(content["country_codes"], serde_json::json!(["US", "GB"]));
    assert_eq!(content["shuffle_options"], true);
    assert_eq!(content["type"]["@type"], "inputPollTypeRegular");
    assert_eq!(content["type"]["allow_adding_options"], true);
    assert_eq!(content["hide_results_until_closes"], true);
    assert_eq!(content["members_only"], true);
    assert_eq!(content["open_period"], 3 * 3600);
    assert_eq!(content["close_date"], 0);
    assert_eq!(content["is_closed"], false);
}

#[test]
fn send_poll_quiz_shape_matches_1_8_67() {
    // `inputPollTypeQuiz` (schema 1.8.67 line 488):
    // `inputPollTypeQuiz correct_option_ids:vector<int32>
    // explanation:formattedText explanation_media:InputPollMedia =
    // InputPollType;` — correct ids are increasing 0-based and
    // non-empty; explanation 0–200 chars with at most 2 line feeds.
    let options = ["Paris", "London"];
    let correct = [0];
    let json = send_poll(
        RequestId(12),
        ChatId(7),
        PollSend {
            question: "Capital of France?",
            options: &options,
            description: "",
            is_anonymous: true,
            allows_multiple_answers: false,
            allows_revoting: false,
            shuffle_options: false,
            country_codes: &[],
            poll_type: PollTypeSend::Quiz {
                correct_option_ids: &correct,
                explanation: "Paris is the capital",
            },
            open_period: 0,
            allow_adding_options: false,
            hide_results_until_closes: false,
            members_only: false,
            close_date: 1_900_000_000,
            reply_to: None,
            topic_id: None,
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let content = &v["input_message_content"];
    assert_eq!(content["@type"], "inputMessagePoll");
    assert!(content["description"].is_null());
    assert_eq!(content["close_date"], 1_900_000_000);
    let quiz = &content["type"];
    assert_eq!(quiz["@type"], "inputPollTypeQuiz");
    assert_eq!(quiz["correct_option_ids"], serde_json::json!([0]));
    assert_eq!(quiz["explanation"]["@type"], "formattedText");
    assert_eq!(quiz["explanation"]["text"], "Paris is the capital");
    assert!(quiz["explanation_media"].is_null());
}
