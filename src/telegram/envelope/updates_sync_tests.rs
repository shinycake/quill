use super::*;

fn payload(json: &str) -> EnvelopePayload {
    parse_envelope(json).unwrap().payload
}

fn live_message(id: i64, chat_id: i64, expires_in: i32, live_period: i32) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat_id},"is_outgoing":true,"date":1700000000,"content":{{"@type":"messageLiveLocation","location":{{"@type":"liveLocation","location":{{"@type":"location","latitude":48.85,"longitude":2.35,"horizontal_accuracy":0}},"live_period":{live_period},"heading":0,"proximity_alert_radius":0}},"expires_in":{expires_in}}}}}"#
    )
}

#[test]
fn default_disable_notification_update_parses() {
    match payload(
        r#"{"@type":"updateChatDefaultDisableNotification","chat_id":-1001,"default_disable_notification":true}"#,
    ) {
        EnvelopePayload::UpdateChatDefaultDisableNotification {
            chat_id,
            default_disable_notification,
        } => {
            assert_eq!(chat_id, ChatId(-1001));
            assert!(default_disable_notification);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn file_download_totals_parse() {
    match payload(
        r#"{"@type":"updateFileDownloads","total_size":"4096","total_count":3,"downloaded_size":1024}"#,
    ) {
        EnvelopePayload::UpdateFileDownloads {
            total_size,
            total_count,
            downloaded_size,
        } => assert_eq!((total_size, total_count, downloaded_size), (4096, 3, 1024)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn file_added_and_removed_from_downloads_parse() {
    let added = r#"{"@type":"updateFileAddedToDownloads","file_download":{"@type":"fileDownload","file_id":77,"message":{"@type":"message","id":5,"chat_id":9,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}},"add_date":1700000100,"complete_date":0,"is_paused":true},"counts":{"@type":"downloadedFileCounts","active_count":1,"paused_count":1,"completed_count":0}}"#;
    match payload(added) {
        EnvelopePayload::UpdateFileAddedToDownloads(download) => {
            assert_eq!(download.file_id, 77);
            assert_eq!(download.add_date, 1_700_000_100);
            assert_eq!(download.complete_date, 0);
            assert!(download.is_paused);
        }
        other => panic!("{other:?}"),
    }
    match payload(
        r#"{"@type":"updateFileRemovedFromDownloads","file_id":77,"counts":{"@type":"downloadedFileCounts","active_count":0,"paused_count":0,"completed_count":0}}"#,
    ) {
        EnvelopePayload::UpdateFileRemovedFromDownloads { file_id } => assert_eq!(file_id, 77),
        other => panic!("{other:?}"),
    }
    // Without a file id there is nothing to track.
    assert!(
        parse_envelope(
            r#"{"@type":"updateFileAddedToDownloads","file_download":{"@type":"fileDownload"}}"#
        )
        .is_err()
    );
}

#[test]
fn dice_emojis_skip_empty_entries() {
    match payload("{\"@type\":\"updateDiceEmojis\",\"emojis\":[\"🎲\",\"\",\"🎯\"]}") {
        EnvelopePayload::UpdateDiceEmojis { emojis } => {
            assert_eq!(emojis, vec!["🎲".to_string(), "🎯".to_string()]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn freeze_state_parses_both_ways() {
    match payload(
        r#"{"@type":"updateFreezeState","is_frozen":true,"freezing_date":1700000000,"deletion_date":1707776000,"appeal_link":"https://t.me/spambot?start=x"}"#,
    ) {
        EnvelopePayload::UpdateFreezeState(state) => {
            assert!(state.is_frozen);
            assert_eq!(state.freezing_date, 1_700_000_000);
            assert_eq!(state.deletion_date, 1_707_776_000);
            assert_eq!(state.appeal_link, "https://t.me/spambot?start=x");
        }
        other => panic!("{other:?}"),
    }
    match payload(r#"{"@type":"updateFreezeState","is_frozen":false}"#) {
        EnvelopePayload::UpdateFreezeState(state) => assert!(!state.is_frozen),
        other => panic!("{other:?}"),
    }
}

#[test]
fn speech_trial_parses() {
    match payload(
        r#"{"@type":"updateSpeechRecognitionTrial","max_media_duration":300,"weekly_count":2,"left_count":1,"next_reset_date":1700600000}"#,
    ) {
        EnvelopePayload::UpdateSpeechRecognitionTrial(trial) => {
            assert_eq!(trial.max_media_duration, 300);
            assert_eq!(trial.weekly_count, 2);
            assert_eq!(trial.left_count, 1);
            assert_eq!(trial.next_reset_date, 1_700_600_000);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn active_live_locations_keep_only_running_shares() {
    let json = format!(
        r#"{{"@type":"updateActiveLiveLocationMessages","messages":[{},{},{}]}}"#,
        live_message(1, 7, 600, 900),
        live_message(2, 8, 0, 900),
        live_message(3, 9, 100, i32::MAX),
    );
    match payload(&json) {
        EnvelopePayload::UpdateActiveLiveLocationMessages { shares } => {
            assert_eq!(shares.len(), 2);
            assert_eq!(shares[0].chat_id, ChatId(7));
            assert_eq!(shares[0].message_id, MessageId(1));
            assert!(shares[0].expires_at > 0 && shares[0].expires_at < i64::MAX);
            assert_eq!(shares[1].expires_at, i64::MAX);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn live_location_viewed_parses() {
    match payload(r#"{"@type":"updateMessageLiveLocationViewed","chat_id":7,"message_id":1048576}"#)
    {
        EnvelopePayload::UpdateMessageLiveLocationViewed {
            chat_id,
            message_id,
        } => assert_eq!((chat_id, message_id), (ChatId(7), MessageId(1_048_576))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn age_verification_parameters_parse_and_clear() {
    match payload(
        r#"{"@type":"updateAgeVerificationParameters","parameters":{"@type":"ageVerificationParameters","min_age":18,"verification_bot_username":"VerifyAgeBot","country":"GB"}}"#,
    ) {
        EnvelopePayload::UpdateAgeVerificationParameters { parameters } => {
            let params = parameters.unwrap();
            assert_eq!(params.min_age, 18);
            assert_eq!(params.verification_bot_username, "VerifyAgeBot");
            assert_eq!(params.country, "GB");
        }
        other => panic!("{other:?}"),
    }
    match payload(r#"{"@type":"updateAgeVerificationParameters"}"#) {
        EnvelopePayload::UpdateAgeVerificationParameters { parameters } => {
            assert!(parameters.is_none());
        }
        other => panic!("{other:?}"),
    }
}
