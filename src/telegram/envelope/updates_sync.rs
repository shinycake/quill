//! Account-level `update*` objects that keep the client in step with the
//! server: the download list, the dice emoji list, the account freeze
//! state, the free speech recognition quota, the running live locations
//! and the age verification parameters (TDLib 1.8.68).
use super::*;
use crate::ids::{ChatId, MessageId};
use serde_json::Value;

/// One running live location of the current user (`updateActiveLiveLocationMessages`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveLiveShare {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    /// Unix time the share stops updating; `i64::MAX` when it never does.
    pub expires_at: i64,
}

/// `ageVerificationParameters` (schema line 10305).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgeVerificationParams {
    pub min_age: i32,
    pub verification_bot_username: String,
    pub country: String,
}

/// `fileDownload` (schema line 3629), reduced to what the list shows.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFileDownload {
    pub file_id: i32,
    pub add_date: i32,
    pub complete_date: i32,
    pub is_paused: bool,
    /// File name of the document the message carries, if any.
    pub name: Option<String>,
    /// Files of the message, so the row can show their size and progress.
    pub files: Vec<ParsedFile>,
}

/// `updateFreezeState` (schema line 11347).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreezeStateUpdate {
    pub is_frozen: bool,
    pub freezing_date: i32,
    pub deletion_date: i32,
    pub appeal_link: String,
}

/// `updateSpeechRecognitionTrial` (schema line 11425).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechTrialUpdate {
    pub max_media_duration: i32,
    pub weekly_count: i32,
    pub left_count: i32,
    pub next_reset_date: i32,
}

pub(crate) fn parse_file_download(value: &Value) -> Option<ParsedFileDownload> {
    let download = value.get("file_download")?;
    let file_id = json_i32(download.get("file_id"), 0);
    if file_id == 0 {
        return None;
    }
    let message = download
        .get("message")
        .and_then(|message| parse_message(message).ok());
    let name = message.as_ref().and_then(|message| match &message.content {
        MessageContent::Document(doc) if !doc.file_name.is_empty() => Some(doc.file_name.clone()),
        _ => None,
    });
    Some(ParsedFileDownload {
        file_id,
        add_date: json_i32(download.get("add_date"), 0),
        complete_date: json_i32(download.get("complete_date"), 0),
        is_paused: json_bool(download.get("is_paused"), false),
        name,
        files: message.map(|message| message.files).unwrap_or_default(),
    })
}

pub(crate) fn parse_active_live_locations(value: &Value) -> Vec<ActiveLiveShare> {
    value
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| parse_message(message).ok())
        .filter_map(|message| match &message.content {
            MessageContent::Location(location) => location.live.and_then(|live| {
                (live.expires_in > 0).then_some(ActiveLiveShare {
                    chat_id: message.chat_id,
                    message_id: message.id,
                    expires_at: if live.live_period == i32::MAX {
                        i64::MAX
                    } else {
                        live.received_at + i64::from(live.expires_in)
                    },
                })
            }),
            _ => None,
        })
        .collect()
}

pub(crate) fn parse_age_verification(value: &Value) -> Option<AgeVerificationParams> {
    let params = value.get("parameters").filter(|p| p.is_object())?;
    Some(AgeVerificationParams {
        min_age: json_i32(params.get("min_age"), 0),
        verification_bot_username: json_field_str(params, "verification_bot_username"),
        country: json_field_str(params, "country"),
    })
}

pub(crate) fn parse_freeze_state(value: &Value) -> FreezeStateUpdate {
    FreezeStateUpdate {
        is_frozen: json_bool(value.get("is_frozen"), false),
        freezing_date: json_i32(value.get("freezing_date"), 0),
        deletion_date: json_i32(value.get("deletion_date"), 0),
        appeal_link: json_field_str(value, "appeal_link"),
    }
}

pub(crate) fn parse_speech_trial(value: &Value) -> SpeechTrialUpdate {
    SpeechTrialUpdate {
        max_media_duration: json_i32(value.get("max_media_duration"), 0),
        weekly_count: json_i32(value.get("weekly_count"), 0),
        left_count: json_i32(value.get("left_count"), 0),
        next_reset_date: json_i32(value.get("next_reset_date"), 0),
    }
}

pub(crate) fn parse_dice_emojis(value: &Value) -> Vec<String> {
    value
        .get("emojis")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|emoji| !emoji.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "updates_sync_tests.rs"]
mod tests;
