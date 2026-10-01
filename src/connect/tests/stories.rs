//! Connect-driver tests: stories.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::story_composer::StoryPrivacy;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn driver_story_reaction_set_remove_and_gates() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Unknown story: rejected.
    assert_eq!(
        driver.set_story_reaction(ChatId(7), 99, Some("❤")),
        Err(ConnectSendError::InvalidRequest)
    );

    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        5,
        "storyContentPhoto",
        r#""chosen_reaction_type":null,"#,
    );
    // Live stories: `setStoryReaction` is not supported for live
    // stories (schema 1.8.67 line 13809).
    seed_story(&mut driver, &seq, &dyn_sink, 7, 6, "storyContentLive", "");
    assert_eq!(
        driver.set_story_reaction(ChatId(7), 6, Some("❤")),
        Err(ConnectSendError::InvalidRequest)
    );
    // Empty emoji: rejected.
    assert_eq!(
        driver.set_story_reaction(ChatId(7), 5, Some("  ")),
        Err(ConnectSendError::InvalidRequest)
    );

    let extra = driver
        .set_story_reaction(ChatId(7), 5, Some("❤"))
        .unwrap()
        .expect("react sends");
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("setStoryReaction");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setStoryReaction");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["story_poster_chat_id"], 7);
    assert_eq!(v["story_id"], 5);
    assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
    assert_eq!(v["reaction_type"]["emoji"], "❤");
    assert_eq!(v["update_recent_reactions"], true);

    // Removing sends `reaction_type: null`.
    let remove_extra = driver
        .set_story_reaction(ChatId(7), 5, None)
        .unwrap()
        .expect("remove sends");
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("remove reaction");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setStoryReaction");
    assert_eq!(v["@extra"], remove_extra.0.to_string());
    assert_eq!(v["reaction_type"], Value::Null);

    // `updateStory` with a chosen reaction refreshes the cache (the
    // viewer reads `chosen_reaction_emoji` live from the cache).
    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        5,
        "storyContentPhoto",
        r#""chosen_reaction_type":{"@type":"reactionTypeEmoji","emoji":"👍"},"#,
    );
    let story = driver.session.stories.get(&(7, 5)).unwrap();
    assert_eq!(story.chosen_reaction_emoji.as_deref(), Some("👍"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_story_custom_emoji_reaction_gates() {
    // Phase 9.2+: the custom-emoji `setStoryReaction` driver mirrors
    // the emoji path's gates plus a positive-id check.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Unknown story: rejected.
    assert_eq!(
        driver.set_story_custom_emoji_reaction(ChatId(7), 99, 123),
        Err(ConnectSendError::InvalidRequest)
    );

    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        5,
        "storyContentPhoto",
        r#""chosen_reaction_type":null,"#,
    );
    // Non-positive custom emoji id: rejected.
    assert_eq!(
        driver.set_story_custom_emoji_reaction(ChatId(7), 5, 0),
        Err(ConnectSendError::InvalidRequest)
    );
    // Live stories: rejected, like the emoji path.
    seed_story(&mut driver, &seq, &dyn_sink, 7, 6, "storyContentLive", "");
    assert_eq!(
        driver.set_story_custom_emoji_reaction(ChatId(7), 6, 123),
        Err(ConnectSendError::InvalidRequest)
    );

    let extra = driver
        .set_story_custom_emoji_reaction(ChatId(7), 5, 123)
        .unwrap()
        .expect("custom emoji reaction sends");
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("setStoryReaction");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setStoryReaction");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["story_poster_chat_id"], 7);
    assert_eq!(v["story_id"], 5);
    assert_eq!(v["reaction_type"]["@type"], "reactionTypeCustomEmoji");
    assert_eq!(v["reaction_type"]["custom_emoji_id"], "123");
    assert_eq!(v["update_recent_reactions"], true);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_delete_story_gated_on_can_be_deleted() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    assert_eq!(
        driver.delete_story(ChatId(7), 5),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
    assert_eq!(
        driver.delete_story(ChatId(7), 5),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        6,
        "storyContentPhoto",
        r#""can_be_deleted":true,"#,
    );
    let extra = driver
        .delete_story(ChatId(7), 6)
        .unwrap()
        .expect("delete sends");
    let json = recorder.snapshot().last().cloned().expect("deleteStory");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteStory");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["story_poster_chat_id"], 7);
    assert_eq!(v["story_id"], 6);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_get_story_interactions_gates_and_dedupes() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Uncached story and a story without `can_get_interactions` are
    // rejected.
    assert_eq!(
        driver.get_story_interactions(ChatId(7), 5, ""),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
    assert_eq!(
        driver.get_story_interactions(ChatId(7), 5, ""),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        6,
        "storyContentPhoto",
        r#""can_get_interactions":true,"#,
    );
    let extra = driver
        .get_story_interactions(ChatId(7), 6, "")
        .unwrap()
        .expect("getStoryInteractions sends");
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("getStoryInteractions");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getStoryInteractions");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["story_id"], 6);
    assert_eq!(v["offset"], "");
    assert_eq!(v["limit"], 50);
    // In-flight fetch dedupes to None (no second request).
    assert_eq!(driver.get_story_interactions(ChatId(7), 6, ""), Ok(None));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_report_story_gates_own_stories() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Uncached and own (deletable) stories are not reportable.
    assert_eq!(
        driver.report_story(ChatId(7), 5, "", ""),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        5,
        "storyContentPhoto",
        r#""can_be_deleted":true,"#,
    );
    assert_eq!(
        driver.report_story(ChatId(7), 5, "", ""),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(&mut driver, &seq, &dyn_sink, 7, 6, "storyContentPhoto", "");
    let extra = driver.report_story(ChatId(7), 6, "", "").unwrap();
    let json = recorder.snapshot().last().cloned().expect("reportStory");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "reportStory");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["story_poster_chat_id"], 7);
    assert_eq!(v["story_id"], 6);
    assert_eq!(v["option_id"], "");
    assert_eq!(v["text"], "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_activate_story_stealth_mode_sends_and_dedupes() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let extra = driver
        .activate_story_stealth_mode()
        .unwrap()
        .expect("activateStoryStealthMode sends");
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("activateStoryStealthMode");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "activateStoryStealthMode");
    assert_eq!(v["@extra"], extra.0.to_string());
    // A second activation while the first is in flight is a no-op.
    assert_eq!(driver.activate_story_stealth_mode(), Ok(None));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_manage_story_gated_on_cached_flags() {
    // Phase 9.5: `editStory` / `editStoryCover` need
    // `can_be_edited`; `setStoryPrivacySettings` needs
    // `can_set_privacy_settings` — both read from the cached
    // story, matching the delete_story gate pattern.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
    assert_eq!(
        driver.edit_story(ChatId(7), 5, None, None, Some("x")),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.edit_story_cover(ChatId(7), 5, 1.0),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.set_story_privacy_settings(ChatId(7), 5, StoryPrivacy::Everyone.settings_json(&[])),
        Err(ConnectSendError::InvalidRequest)
    );

    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        6,
        "storyContentPhoto",
        r#""can_be_edited":true,"can_set_privacy_settings":true,"#,
    );
    let extra = driver
        .edit_story(ChatId(7), 6, None, None, Some("new"))
        .unwrap();
    let v: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(v["@type"], "editStory");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert!(v["content"].is_null());
    assert!(v["areas"].is_null());
    assert_eq!(v["caption"]["text"], "new");
    assert!(driver.session.story_manage.pending);

    let extra = driver.edit_story_cover(ChatId(7), 6, 2.5).unwrap();
    let v: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(v["@type"], "editStoryCover");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["cover_frame_timestamp"], 2.5);
    assert!(driver.session.story_manage.pending);

    let extra = driver
        .set_story_privacy_settings(ChatId(7), 6, StoryPrivacy::Contacts.settings_json(&[]))
        .unwrap();
    let v: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(v["@type"], "setStoryPrivacySettings");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(
        v["privacy_settings"]["@type"],
        "storyPrivacySettingsContacts"
    );
    assert!(driver.session.story_manage.pending);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_send_story_reply_uses_input_message_reply_to_story() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
    // Not repliable and empty text are rejected.
    assert_eq!(
        driver.send_story_reply(ChatId(7), 5, "hello"),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        6,
        "storyContentPhoto",
        r#""can_be_replied":true,"#,
    );
    assert_eq!(
        driver.send_story_reply(ChatId(7), 6, "   "),
        Err(ConnectSendError::InvalidRequest)
    );

    let extra = driver
        .send_story_reply(ChatId(7), 6, "Nice story!")
        .unwrap();
    let json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
    assert_eq!(v["reply_to"]["story_poster_chat_id"], 7);
    assert_eq!(v["reply_to"]["story_id"], 6);
    assert_eq!(v["input_message_content"]["text"]["text"], "Nice story!");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_get_story_available_reactions_dedupes_and_caches() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let extra = driver
        .get_story_available_reactions()
        .unwrap()
        .expect("first call sends");
    let sent = recorder.snapshot().len();
    // In-flight duplicate is deduped.
    assert!(driver.get_story_available_reactions().unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent);

    driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"availableReactions","@extra":"{}","top_reactions":[{{"@type":"availableReaction","type":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"needs_premium":false}}],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":false,"are_tags":false,"unavailability_reason":null}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let cached = driver.session.story_available_reactions.clone().unwrap();
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].emoji, "❤");
    // Cached: no new request.
    assert!(driver.get_story_available_reactions().unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_pin_and_unpin_chat_message_then_is_pinned_update() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let extra = driver
        .pin_chat_message(ChatId(7), MessageId(50), false)
        .unwrap();
    let json = recorder.snapshot().last().cloned().expect("pinChatMessage");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "pinChatMessage");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 50);
    assert_eq!(v["disable_notification"], false);
    assert_eq!(v["only_for_self"], false);
    assert!(!json.contains("unpinAllChatMessages"));

    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateMessageIsPinned","chat_id":7,"message_id":50,"is_pinned":true}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let pinned = driver
        .session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(pinned.is_pinned);

    let unpin_extra = driver.unpin_chat_message(ChatId(7), MessageId(50)).unwrap();
    let unpin_json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("unpinChatMessage");
    let v: Value = serde_json::from_str(&unpin_json).unwrap();
    assert_eq!(v["@type"], "unpinChatMessage");
    assert_eq!(v["@extra"], unpin_extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 50);

    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, unpin_extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageIsPinned","chat_id":7,"message_id":50,"is_pinned":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let cleared = driver
        .session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(!cleared.is_pinned);
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// stories-live-play: `join_live_story` sends `getGroupCall` for the live
/// story's group call and records the pending join; the ingest pump then
/// issues `joinLiveStory` once the `groupCall` answer has created the
/// unjoined tracker. Gates: unknown story, non-live story, and a second
/// join while one is pending are all refused.
#[test]
fn driver_join_live_story_two_step_and_gates() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Unknown story: rejected.
    assert_eq!(
        driver.join_live_story(ChatId(7), 99),
        Err(ConnectSendError::InvalidRequest)
    );

    // Non-live story: rejected.
    seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
    assert_eq!(
        driver.join_live_story(ChatId(7), 5),
        Err(ConnectSendError::InvalidRequest)
    );

    // Live story (group_call_id rides in the content object): sends
    // getGroupCall and records the pending join.
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"story","id":6,"poster_chat_id":7,"date":1700000000,"content":{"@type":"storyContentLive","group_call_id":4242,"is_rtmp_stream":false},"caption":{"@type":"formattedText","text":"","entities":[]}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    // An existing tracked call refuses a competing live-story join.
    driver.session.active_group_call = Some(tracked_group_call(99, false, false));
    assert_eq!(
        driver.join_live_story(ChatId(7), 6),
        Err(ConnectSendError::InvalidRequest)
    );
    driver.session.active_group_call = None;
    // Failed fetch clears the intent so the next tap can retry.
    let failed = driver.join_live_story(ChatId(7), 6).unwrap();
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","@extra":"{}","code":400,"message":"GROUPCALL_INVALID"}}"#,
                    failed.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.pending_live_story_join, None);
    let extra = driver.join_live_story(ChatId(7), 6).unwrap();
    let get = sent_request(&recorder, "getGroupCall");
    assert_eq!(get["group_call_id"], 4242);
    assert_eq!(get["@extra"], extra.0.to_string());
    let intent = driver
        .session
        .pending_live_story_join
        .expect("pending live-story join recorded");
    assert_eq!(intent.group_call_id, 4242);
    assert_eq!(intent.request, extra);

    // Second join while one is pending: rejected.
    assert_eq!(
        driver.join_live_story(ChatId(7), 6),
        Err(ConnectSendError::InvalidRequest)
    );

    // The `groupCall` answer creates the unjoined tracker; the pump then
    // issues `joinLiveStory` for the pending live-story join (the no-device
    // fallback params — no call engine in tests).
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":4242,"title":"Live","is_active":true,"is_video_chat":true,"is_live_story":true,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"participant_count":3}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    // One more ingest in case the answer applied after the pump section.
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateOption","name":"my_id","value":{"@type":"optionValueInteger","value":1}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let join = sent_request(&recorder, "joinLiveStory");
    assert_eq!(join["group_call_id"], 4242);
    assert_eq!(driver.session.pending_live_story_join, None);
    assert!(
        driver
            .session
            .active_group_call
            .as_ref()
            .unwrap()
            .is_live_story
    );
    // Reconnect uses the same live-story method, not joinVideoChat.
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .reconnecting = true;
    driver.rejoin_group_call(false).unwrap();
    let retry = sent_request(&recorder, "joinLiveStory");
    assert_eq!(retry["group_call_id"], 4242);
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|json| !json.contains("joinVideoChat"))
    );

    let _ = std::fs::remove_dir_all(&dir);
}
