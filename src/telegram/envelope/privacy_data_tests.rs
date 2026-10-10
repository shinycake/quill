use super::*;

/// B13: `sessions.inactive_session_ttl_days` and the details-box fields
/// (`log_in_date`, `is_official_application`) parse; an absent or zero TTL
/// stays unknown instead of overwriting a known value.
#[test]
fn sessions_carry_the_inactive_ttl_and_detail_fields() {
    let json = r#"{"@type":"sessions","inactive_session_ttl_days":90,"sessions":[
{"@type":"session","id":"11","is_current":true,"log_in_date":1700000000,"is_official_application":true,"application_name":"Quill","device_model":"Mac"}
]}"#;
    match parse_envelope(json).unwrap().payload {
        EnvelopePayload::Settings(SettingsPayload::Sessions {
            sessions,
            inactive_session_ttl_days,
        }) => {
            assert_eq!(inactive_session_ttl_days, Some(90));
            assert_eq!(sessions[0].log_in_date, 1_700_000_000);
            assert!(sessions[0].is_official_application);
        }
        other => panic!("{other:?}"),
    }
    for absent in [
        r#"{"@type":"sessions","sessions":[]}"#,
        r#"{"@type":"sessions","inactive_session_ttl_days":0,"sessions":[]}"#,
    ] {
        match parse_envelope(absent).unwrap().payload {
            EnvelopePayload::Settings(SettingsPayload::Sessions {
                inactive_session_ttl_days,
                ..
            }) => assert_eq!(inactive_session_ttl_days, None),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn b13_answers_parse() {
    match parse_envelope(
        r#"{"@type":"newChatPrivacySettings","allow_new_chats_from_unknown_users":false,"incoming_paid_message_star_count":3}"#,
    )
    .unwrap()
    .payload
    {
        EnvelopePayload::Settings(SettingsPayload::NewChatPrivacySettings(settings)) => {
            assert!(!settings.allow_from_unknown);
            assert_eq!(settings.incoming_paid_message_star_count, 3);
        }
        other => panic!("{other:?}"),
    }
    match parse_envelope(
        r#"{"@type":"networkStatistics","since_date":7,"entries":[{"@type":"networkStatisticsEntryFile","file_type":{"@type":"fileTypeAudio"},"network_type":{"@type":"networkTypeMobile"},"sent_bytes":1,"received_bytes":2}]}"#,
    )
    .unwrap()
    .payload
    {
        EnvelopePayload::Settings(SettingsPayload::NetworkStatistics(usage)) => {
            assert_eq!(usage.since_date, 7);
            assert_eq!(usage.grand_total().total(), 3);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        parse_envelope(r#"{"@type":"recoveryEmailAddress","recovery_email_address":""}"#)
            .unwrap()
            .payload,
        EnvelopePayload::Settings(SettingsPayload::RecoveryEmailAddress)
    ));
    match parse_envelope(
        r#"{"@type":"updateSuggestedActions","added_actions":[{"@type":"suggestedActionCheckPassword"},{"@type":"suggestedActionUpgradePremium"}],"removed_actions":[]}"#,
    )
    .unwrap()
    .payload
    {
        EnvelopePayload::Settings(SettingsPayload::UpdateSuggestedActions { added, removed }) => {
            assert_eq!(
                added,
                ["suggestedActionCheckPassword", "suggestedActionUpgradePremium"]
            );
            assert!(removed.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn user_full_info_reads_gift_settings() {
    match parse_envelope(
        r#"{"@type":"updateUserFullInfo","user_id":5,"user_full_info":{"@type":"userFullInfo","gift_settings":{"@type":"giftSettings","show_gift_button":true,"accepted_gift_types":{"@type":"acceptedGiftTypes","unlimited_gifts":true,"limited_gifts":true,"upgraded_gifts":false,"gifts_from_channels":true,"premium_subscription":true}}}}"#,
    )
    .unwrap()
    .payload
    {
        EnvelopePayload::Users(UsersPayload::UpdateUserFullInfo { extras, .. }) => {
            let gifts = extras.gift_settings.expect("gift settings");
            assert!(gifts.show_gift_button && !gifts.upgraded_gifts);
        }
        other => panic!("{other:?}"),
    }
    match parse_envelope(
        r#"{"@type":"updateUserFullInfo","user_id":5,"user_full_info":{"@type":"userFullInfo"}}"#,
    )
    .unwrap()
    .payload
    {
        EnvelopePayload::Users(UsersPayload::UpdateUserFullInfo { extras, .. }) => {
            assert!(extras.gift_settings.is_none())
        }
        other => panic!("{other:?}"),
    }
}
