//! The composer's silent and scheduled choices on media sends. The media
//! builders write `options: null`; this puts the send options on a
//! finished `sendMessage` or `sendMessageAlbum` request body.

use crate::composer::{ComposerScheduling, SendOptions};
use serde_json::Value;

/// Set `options` on a built send request when the user chose to send
/// silently or on a schedule. `single` is false for `sendMessageAlbum`:
/// TDLib accepts a repeat period only on `sendMessage` (and a one-message
/// forward), so an album is sent without one.
pub fn with_message_send_options(json: String, options: &SendOptions, single: bool) -> String {
    let wanted = options.disable_notification
        || options.scheduling != ComposerScheduling::None
        || options.effect_id != 0;
    if !wanted {
        return json;
    }
    let Ok(mut value) = serde_json::from_str::<Value>(&json) else {
        return json;
    };
    let options = SendOptions {
        scheduling: if single {
            options.scheduling
        } else {
            options.scheduling.repeating(0)
        },
        ..*options
    };
    crate::json_access::set(&mut value, "options", super::message_send_options(&options));
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::with_message_send_options;
    use crate::composer::{ComposerScheduling, SendOptions};
    use serde_json::{Value, json};

    fn body() -> String {
        json!({ "@type": "sendMessage", "chat_id": 1, "options": Value::Null }).to_string()
    }

    #[test]
    fn plain_sends_are_left_alone() {
        assert_eq!(
            with_message_send_options(body(), &SendOptions::default(), true),
            body()
        );
    }

    #[test]
    fn a_schedule_and_silence_reach_a_media_send() {
        let options = SendOptions {
            disable_notification: true,
            scheduling: ComposerScheduling::at(2_000_000_000).repeating(86_400),
            ..SendOptions::default()
        };
        let v: Value =
            serde_json::from_str(&with_message_send_options(body(), &options, true)).unwrap();
        assert_eq!(v["options"]["disable_notification"], true);
        assert_eq!(
            v["options"]["scheduling_state"]["@type"],
            "messageSchedulingStateSendAtDate"
        );
        assert_eq!(v["options"]["scheduling_state"]["repeat_period"], 86_400);
        assert_eq!(v["chat_id"], 1);
    }

    #[test]
    fn an_album_never_carries_a_repeat_period() {
        let options = SendOptions {
            scheduling: ComposerScheduling::at(2_000_000_000).repeating(86_400),
            ..SendOptions::default()
        };
        let v: Value =
            serde_json::from_str(&with_message_send_options(body(), &options, false)).unwrap();
        assert_eq!(v["options"]["scheduling_state"]["send_date"], 2_000_000_000);
        assert_eq!(v["options"]["scheduling_state"]["repeat_period"], 0);
    }

    #[test]
    fn a_date_past_2038_saturates_instead_of_wrapping_to_the_past() {
        let far = i64::from(i32::MAX) + 10;
        let json = super::super::messages::message_send_options(&SendOptions {
            scheduling: ComposerScheduling::at(far),
            ..SendOptions::default()
        });
        assert_eq!(json["scheduling_state"]["send_date"], i32::MAX);
    }
}
