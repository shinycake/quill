use crate::composer::{ComposerScheduling, SendOptions};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests::{
    forward_messages, forward_messages_with_options, get_chat_available_message_senders,
    search_chats_on_server, set_chat_message_sender,
};
use serde_json::Value;

fn parse(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

#[test]
fn available_senders_and_set_sender_shapes() {
    let v = parse(&get_chat_available_message_senders(
        RequestId(1),
        ChatId(-100),
    ));
    assert_eq!(v["@type"], "getChatAvailableMessageSenders");
    assert_eq!(v["chat_id"], -100);
    let v = parse(&set_chat_message_sender(
        RequestId(2),
        ChatId(-100),
        MessageSender::Chat { chat_id: -200 },
    ));
    assert_eq!(v["@type"], "setChatMessageSender");
    assert_eq!(v["message_sender_id"]["@type"], "messageSenderChat");
    assert_eq!(v["message_sender_id"]["chat_id"], -200);
    let v = parse(&set_chat_message_sender(
        RequestId(3),
        ChatId(-100),
        MessageSender::User { user_id: 5 },
    ));
    assert_eq!(v["message_sender_id"]["@type"], "messageSenderUser");
    assert_eq!(v["message_sender_id"]["user_id"], 5);
}

#[test]
fn search_chats_on_server_shape() {
    let v = parse(&search_chats_on_server(RequestId(4), "ada", 30));
    assert_eq!(v["@type"], "searchChatsOnServer");
    assert_eq!(v["query"], "ada");
    assert_eq!(v["limit"], 30);
    assert_eq!(v["type_filter"], Value::Null);
}

#[test]
fn forward_options_carry_silent_and_schedule() {
    let plain = parse(&forward_messages_with_options(
        RequestId(5),
        ChatId(1),
        ChatId(2),
        &[MessageId(3)],
        false,
        false,
        &SendOptions::default(),
    ));
    assert_eq!(plain["options"], Value::Null);
    let plain_old = parse(&forward_messages(
        RequestId(5),
        ChatId(1),
        ChatId(2),
        &[MessageId(3)],
        false,
        false,
    ));
    assert_eq!(plain, plain_old);
    let silent = parse(&forward_messages_with_options(
        RequestId(6),
        ChatId(1),
        ChatId(2),
        &[MessageId(3)],
        true,
        false,
        &SendOptions {
            disable_notification: true,
            ..SendOptions::default()
        },
    ));
    assert_eq!(silent["options"]["@type"], "messageSendOptions");
    assert_eq!(silent["options"]["disable_notification"], true);
    let scheduled = parse(&forward_messages_with_options(
        RequestId(7),
        ChatId(1),
        ChatId(2),
        &[MessageId(3)],
        false,
        false,
        &SendOptions {
            scheduling: ComposerScheduling::SendAtDate(1_900_000_000),
            ..SendOptions::default()
        },
    ));
    assert_eq!(
        scheduled["options"]["scheduling_state"]["@type"],
        "messageSchedulingStateSendAtDate"
    );
    assert_eq!(
        scheduled["options"]["scheduling_state"]["send_date"],
        1_900_000_000
    );
}
