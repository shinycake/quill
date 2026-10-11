//! Message statistics, public forwards of a post and async / zoomed
//! graph request builders (TDLib 1.8.68, `schema/td_api.tl`).

use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::json;

/// `getMessageStatistics chat_id:int53 message_id:int53 is_dark:Bool =
/// MessageStatistics` (schema line 16355). Only valid when
/// `messageProperties.can_get_statistics`.
pub fn get_message_statistics(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    is_dark: bool,
) -> String {
    json!({
        "@type": "getMessageStatistics",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "is_dark": is_dark
    })
    .to_string()
}

/// `getMessagePublicForwards chat_id:int53 message_id:int53 offset:string
/// limit:int32 = PublicForwards` (schema line 16362). TDLib may return
/// fewer than `limit`; the page ends when `next_offset` is empty.
pub fn get_message_public_forwards(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getMessagePublicForwards",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "offset": offset,
        "limit": limit.clamp(1, 100)
    })
    .to_string()
}

/// `getStatisticalGraph chat_id:int53 token:string x:int53 =
/// StatisticalGraph` (schema line 16368): an async graph (`x` 0) or one
/// day of a graph zoomed in at `x` (milliseconds, from the graph's x
/// column).
pub fn get_statistical_graph(extra: RequestId, chat_id: ChatId, token: &str, x: i64) -> String {
    json!({
        "@type": "getStatisticalGraph",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "token": token,
        "x": x
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn message_statistics_request() {
        let v = parse(&get_message_statistics(
            RequestId(7),
            ChatId(-100),
            MessageId(1 << 20),
            true,
        ));
        assert_eq!(v["@type"], "getMessageStatistics");
        assert_eq!(v["@extra"], "7");
        assert_eq!(v["chat_id"], -100);
        assert_eq!(v["message_id"], 1 << 20);
        assert_eq!(v["is_dark"], true);
    }

    #[test]
    fn public_forwards_request_clamps_the_limit() {
        let v = parse(&get_message_public_forwards(
            RequestId(8),
            ChatId(-100),
            MessageId(5),
            "next",
            500,
        ));
        assert_eq!(v["@type"], "getMessagePublicForwards");
        assert_eq!(v["offset"], "next");
        assert_eq!(v["limit"], 100);
    }

    #[test]
    fn statistical_graph_request() {
        let v = parse(&get_statistical_graph(
            RequestId(9),
            ChatId(-100),
            "tok",
            1_727_740_800_000,
        ));
        assert_eq!(v["@type"], "getStatisticalGraph");
        assert_eq!(v["token"], "tok");
        assert_eq!(v["x"], 1_727_740_800_000i64);
    }
}
