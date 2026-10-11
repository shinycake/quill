//! State reducer tests: account-level sync updates.
use super::common::*;
use super::*;
use crate::telegram::envelope::{ActiveLiveShare, SpeechTrialUpdate};

fn apply(session: &mut Session, json: &str) {
    // The reducer drops envelopes that are not newer than the last one.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let sink = Arc::new(MemorySink::new());
    apply_json(session, &SEQ, &sink, json);
}

#[test]
fn default_silent_follows_the_update() {
    let (mut session, _sink) = session();
    assert!(!session.sync.is_default_silent(5));
    apply(
        &mut session,
        r#"{"@type":"updateChatDefaultDisableNotification","chat_id":5,"default_disable_notification":true}"#,
    );
    assert!(session.sync.is_default_silent(5));
    apply(
        &mut session,
        r#"{"@type":"updateChatDefaultDisableNotification","chat_id":5,"default_disable_notification":false}"#,
    );
    assert!(!session.sync.is_default_silent(5));
}

#[test]
fn download_added_elsewhere_enters_the_list_and_removal_leaves_it() {
    let (mut session, _sink) = session();
    let added = |id: i32, complete: i32, paused: bool| {
        format!(
            r#"{{"@type":"updateFileAddedToDownloads","file_download":{{"@type":"fileDownload","file_id":{id},"message":{{"@type":"message","id":5,"chat_id":9,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}},"add_date":1700000100,"complete_date":{complete},"is_paused":{paused}}},"counts":{{"@type":"downloadedFileCounts","active_count":1,"paused_count":0,"completed_count":0}}}}"#
        )
    };
    apply(&mut session, &added(40, 0, false));
    assert!(session.media.user_downloads.contains(&40));
    assert!(!session.media.paused_downloads.contains(&40));
    apply(&mut session, &added(41, 0, true));
    assert!(session.media.paused_downloads.contains(&41));
    apply(&mut session, &added(42, 1_700_000_200, false));
    assert!(!session.media.user_downloads.contains(&42));
    assert!(session.media.completed_downloads.contains(&42));

    apply(
        &mut session,
        r#"{"@type":"updateFileRemovedFromDownloads","file_id":41,"counts":{"@type":"downloadedFileCounts","active_count":1,"paused_count":0,"completed_count":1}}"#,
    );
    assert!(!session.media.user_downloads.contains(&41));
    assert!(!session.media.paused_downloads.contains(&41));
    apply(
        &mut session,
        r#"{"@type":"updateFileRemovedFromDownloads","file_id":42,"counts":{"@type":"downloadedFileCounts","active_count":1,"paused_count":0,"completed_count":0}}"#,
    );
    assert!(!session.media.completed_downloads.contains(&42));
}

#[test]
fn download_totals_summary_and_empty_list() {
    let (mut session, _sink) = session();
    apply(
        &mut session,
        r#"{"@type":"updateFileDownloads","total_size":2048,"total_count":2,"downloaded_size":1024}"#,
    );
    let summary = session.sync.download_summary(|n| format!("{n} B")).unwrap();
    assert_eq!(summary, "2 files \u{b7} 1024 B of 2048 B");
    apply(
        &mut session,
        r#"{"@type":"updateFileDownloads","total_size":0,"total_count":0,"downloaded_size":0}"#,
    );
    assert!(session.sync.download_summary(|n| n.to_string()).is_none());
}

#[test]
fn dice_menu_falls_back_until_the_server_sends_a_list() {
    let (mut session, _sink) = session();
    assert_eq!(session.sync.dice_menu().len(), DEFAULT_DICE_EMOJIS.len());
    apply(
        &mut session,
        "{\"@type\":\"updateDiceEmojis\",\"emojis\":[\"🎲\",\"🎳\"]}",
    );
    assert_eq!(
        session.sync.dice_menu(),
        vec!["🎲".to_string(), "🎳".to_string()]
    );
}

#[test]
fn freeze_state_sets_and_clears_the_restriction() {
    let (mut session, _sink) = session();
    assert!(!session.is_frozen());
    apply(
        &mut session,
        r#"{"@type":"updateFreezeState","is_frozen":true,"freezing_date":1700000000,"deletion_date":1707776000,"appeal_link":"https://t.me/spambot"}"#,
    );
    assert!(session.is_frozen());
    let info = session.sync.freeze.clone().unwrap();
    assert_eq!(info.appeal_link, "https://t.me/spambot");
    assert_eq!(
        freeze_deadline_label(&info, 1_700_000_000),
        "February 12, 2024"
    );
    apply(
        &mut session,
        r#"{"@type":"updateFreezeState","is_frozen":false}"#,
    );
    assert!(!session.is_frozen());
}

#[test]
fn speech_trial_hint_counts_and_runs_out() {
    let (mut session, _sink) = session();
    apply(
        &mut session,
        r#"{"@type":"updateSpeechRecognitionTrial","max_media_duration":300,"weekly_count":2,"left_count":1,"next_reset_date":1700600000}"#,
    );
    let trial = session.sync.speech_trial.unwrap();
    assert_eq!(
        speech_trial_hint(&trial, 1_700_000_000),
        "You have 1 free transcription left until November 21."
    );
    let more = SpeechTrialUpdate {
        left_count: 2,
        ..trial
    };
    assert!(speech_trial_hint(&more, 1_700_000_000).contains("2 free transcriptions left"));
    let out = SpeechTrialUpdate {
        left_count: 0,
        ..trial
    };
    assert_eq!(
        speech_trial_hint(&out, 1_700_000_000),
        "You have used all your free transcriptions this week. Wait until November 21 to use it again or subscribe to Premium now."
    );
}

#[test]
fn live_shares_replace_keep_viewed_and_expire() {
    let (mut session, _sink) = session();
    let share = |chat: i64, msg: i64, at: i64| ActiveLiveShare {
        chat_id: ChatId(chat),
        message_id: MessageId(msg),
        expires_at: at,
    };
    session
        .sync
        .set_live_shares(vec![share(7, 1, 2_000), share(8, 2, i64::MAX)]);
    apply(
        &mut session,
        r#"{"@type":"updateMessageLiveLocationViewed","chat_id":7,"message_id":1}"#,
    );
    assert!(session.sync.live_shares[0].viewed);
    // The next update keeps the viewed mark of shares that continue.
    session
        .sync
        .set_live_shares(vec![share(7, 1, 2_000), share(9, 3, 500)]);
    assert!(session.sync.live_shares[0].viewed);
    assert!(!session.sync.live_shares[1].viewed);
    assert_eq!(session.sync.live_shares_at(1_000).count(), 1);
    assert_eq!(session.sync.live_shares_at(100).count(), 2);
    // An empty update ends every share.
    session.sync.set_live_shares(Vec::new());
    assert_eq!(session.sync.live_shares_at(0).count(), 0);
}

#[test]
fn effective_silent_honors_chat_default_and_override() {
    assert!(!effective_silent(false, false, false));
    assert!(effective_silent(true, false, false));
    assert!(effective_silent(false, true, false));
    assert!(!effective_silent(false, true, true));
    assert!(effective_silent(true, true, true));
}

#[test]
fn age_gate_only_blocks_turning_on_before_verification() {
    assert!(age_gate_blocks(true, true, false));
    assert!(!age_gate_blocks(false, true, false));
    assert!(!age_gate_blocks(true, false, false));
    assert!(!age_gate_blocks(true, true, true));
}

#[test]
fn live_strip_copy() {
    assert_eq!(live_strip_label(0), "");
    assert_eq!(live_strip_label(1), "Sharing your live location");
    assert_eq!(live_strip_label(3), "Sharing your live location in 3 chats");
    assert_eq!(live_left_label(i64::MAX, 0), "No time limit");
    assert_eq!(live_left_label(5_400, 0), "1 h 30 min left");
    assert_eq!(live_left_label(601, 0), "11 min left");
    assert_eq!(live_left_label(30, 0), "30 s left");
    assert_eq!(live_left_label(10, 50), "0 s left");
}

#[test]
fn age_verification_parameters_drive_the_prompt() {
    let (mut session, _sink) = session();
    assert!(session.sync.age_verification.is_none());
    apply(
        &mut session,
        r#"{"@type":"updateAgeVerificationParameters","parameters":{"@type":"ageVerificationParameters","min_age":18,"verification_bot_username":"@VerifyAgeBot","country":"GB"}}"#,
    );
    let params = session.sync.age_verification.clone().unwrap();
    assert_eq!(
        age_verification_about(&params),
        "To view this content you need to confirm you are 18 or older."
    );
    assert_eq!(
        age_verification_bot_url(&params).as_deref(),
        Some("https://t.me/VerifyAgeBot")
    );
    apply(
        &mut session,
        r#"{"@type":"updateAgeVerificationParameters"}"#,
    );
    assert!(session.sync.age_verification.is_none());
}
