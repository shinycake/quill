use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::{Value, json};

/// `addPollOption` (TDLib 1.8.67, `schema/td_api.tl:12920`):
/// `addPollOption chat_id:int53 message_id:int53 option:inputPollOption =
/// Ok;`. Only valid when `messagePoll.can_add_option`; `media` is null
/// (option media is out of this slice). The new option arrives through
/// `updatePoll`.
pub fn add_poll_option(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    text: &str,
) -> String {
    json!({
        "@type": "addPollOption",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "option": {
            "@type": "inputPollOption",
            "text": { "@type": "formattedText", "text": text, "entities": [] },
            "media": Value::Null
        }
    })
    .to_string()
}

/// `getPollVoteStatistics` (TDLib 1.8.67, `schema/td_api.tl:12947`):
/// `getPollVoteStatistics chat_id:int53 message_id:int53 is_dark:Bool =
/// PollVoteStatistics;`. Only valid when
/// `messageProperties.can_get_poll_vote_statistics`.
pub fn get_poll_vote_statistics(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    is_dark: bool,
) -> String {
    json!({
        "@type": "getPollVoteStatistics",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "is_dark": is_dark,
    })
    .to_string()
}

/// `readAllChatPollVotes` (TDLib 1.8.67, `schema/td_api.tl:13308`):
/// `readAllChatPollVotes chat_id:int53 = Ok;`.
pub fn read_all_chat_poll_votes(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "readAllChatPollVotes",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn add_option_shape() {
        let value = parse(&add_poll_option(
            RequestId(2),
            ChatId(7),
            MessageId(9),
            "Tacos",
        ));
        assert_eq!(value["@type"], "addPollOption");
        assert_eq!(value["chat_id"], 7);
        assert_eq!(value["message_id"], 9);
        assert_eq!(value["option"]["@type"], "inputPollOption");
        assert_eq!(value["option"]["text"]["text"], "Tacos");
        assert!(value["option"]["media"].is_null());
    }

    #[test]
    fn statistics_and_read_all_shapes() {
        let value = parse(&get_poll_vote_statistics(
            RequestId(2),
            ChatId(7),
            MessageId(9),
            true,
        ));
        assert_eq!(value["@type"], "getPollVoteStatistics");
        assert_eq!(value["is_dark"], true);
        let value = parse(&read_all_chat_poll_votes(RequestId(2), ChatId(7)));
        assert_eq!(value["@type"], "readAllChatPollVotes");
        assert_eq!(value["chat_id"], 7);
    }
}
