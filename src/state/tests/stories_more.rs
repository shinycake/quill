//! State reducer tests: stories (continued from stories.rs).
use super::common::*;
use super::*;

#[test]
fn story_report_flow_through_option_and_text_steps() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_story_report(12, 6);
    assert!(matches!(
        session.stories.report.as_ref().unwrap().stage,
        StoryReportStage::Checking
    ));
    let extra = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"reportStoryResultOptionRequired","@extra":"{}","title":"Why?","options":[{{"@type":"reportOption","id":"aGk=","text":"Spam"}}]}}"#,
            extra.0
        ),
    );
    match &session.stories.report.as_ref().unwrap().stage {
        StoryReportStage::PickOption { title, options } => {
            assert_eq!(title, "Why?");
            assert_eq!(options[0].id, "aGk=");
        }
        other => panic!("unexpected {other:?}"),
    }
    session.story_report_sending(12, 6);
    let extra2 = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"reportStoryResultTextRequired","@extra":"{}","option_id":"aGk=","is_optional":false}}"#,
            extra2.0
        ),
    );
    assert!(matches!(
        session.stories.report.as_ref().unwrap().stage,
        StoryReportStage::TextRequired { ref option_id, .. } if option_id == "aGk="
    ));
    let extra3 = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"reportStoryResultOk","@extra":"{}"}}"#,
            extra3.0
        ),
    );
    assert!(matches!(
        session.stories.report.as_ref().unwrap().stage,
        StoryReportStage::Reported
    ));
    // A late error after the flow closed does not resurrect it.
    session.clear_story_report();
    let extra4 = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"LATE"}}"#,
            extra4.0
        ),
    );
    assert!(session.stories.report.is_none());
}

#[test]
fn story_report_empty_options_means_reported() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_story_report(12, 6);
    let extra = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"reportStoryResultOptionRequired","@extra":"{}","title":"Why?","options":[]}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.stories.report.as_ref().unwrap().stage,
        StoryReportStage::Reported
    ));
}

#[test]
fn story_report_send_failure_ends_flow() {
    let (mut session, _sink) = session();
    session.begin_story_report(12, 6);
    session.fail_story_report_send(12, 6, "could not report story".into());
    assert!(matches!(
        session.stories.report.as_ref().unwrap().stage,
        StoryReportStage::Failed(_)
    ));
    // A different story's flow is untouched.
    session.begin_story_report(12, 7);
    session.fail_story_report_send(12, 6, "could not report story".into());
    assert!(matches!(
        session.stories.report.as_ref().unwrap().stage,
        StoryReportStage::Checking
    ));
}

#[test]
fn update_story_stealth_mode_stored() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateStoryStealthMode","active_until_date":1700003600,"cooldown_until_date":1700007200}"#,
    );
    assert_eq!(
        session.stories.stealth,
        StoryStealthMode {
            active_until_date: 1700003600,
            cooldown_until_date: 1700007200,
        }
    );
    assert!(session.stories.stealth.is_active(1700000000));
    assert!(!session.stories.stealth.is_active(1700003600));
    assert!(session.stories.stealth.is_cooling_down(1700003600));
    assert!(!session.stories.stealth.is_cooling_down(1700007200));
}

#[test]
fn update_story_post_succeeded_upserts_and_queues_tray_refresh() {
    // Phase 9.2: `updateStoryPostSucceeded` (schema 1.8.67 line 10901).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateStoryPostSucceeded","story":{"@type":"story","id":9,"poster_chat_id":11,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"old_story_id":8}"#,
    );
    let story = session.stories.stories.get(&(11, 9)).expect("story cached");
    assert_eq!(story.poster_chat_id, 11);
    // The driver's `tick` drains this into a `getChatActiveStories`
    // refresh for the poster's tray entry.
    assert!(session.stories.tray_refresh.contains(&11));
}

#[test]
fn story_post_outcome_transitions() {
    // Phase 9.3: the composer's honest pending / succeeded / failed
    // states, driven by purpose-gated reducers.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);

    // `canPostStory` answer (purpose-gated into `story_post.eligibility`).
    let extra = session.request(RequestPurpose::CheckCanPostStory, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"canPostStoryResultWeeklyLimitExceeded","@extra":"{}","retry_after":9000}}"#,
            extra.0
        ),
    );
    let eligibility = session
        .stories
        .post
        .eligibility
        .clone()
        .expect("eligibility stored");
    assert!(!eligibility.can_post());
    assert!(eligibility.user_message().contains("2h 30m"));

    // A stray result with no matching pending purpose is ignored.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"canPostStoryResultOk","story_count":1}"#,
    );
    assert!(!session.stories.post.eligibility.clone().unwrap().can_post());

    // `postStory` answer → Posting with the temporary story id.
    let extra = session.request(RequestPurpose::PostStory, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"story","@extra":"{}","id":8,"poster_chat_id":777,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.stories.post.outcome,
        StoryPostOutcome::Posting { story_id: 8 }
    );

    // `updateStoryPostSucceeded` with a matching old_story_id →
    // Succeeded (and the 9.2 upsert still runs).
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateStoryPostSucceeded","story":{"@type":"story","id":9,"poster_chat_id":777,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"old_story_id":8}"#,
    );
    assert_eq!(session.stories.post.outcome, StoryPostOutcome::Succeeded);
    assert!(session.stories.stories.contains_key(&(777, 9)));

    // Failed path: new pending post, then `updateStoryPostFailed`.
    let extra = session.request(RequestPurpose::PostStory, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"story","@extra":"{}","id":10,"poster_chat_id":777,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
            extra.0
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateStoryPostFailed","story":{"@type":"story","id":10,"poster_chat_id":777,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"error":{"@type":"error","code":400,"message":"x"},"error_type":{"@type":"canPostStoryResultOk"}}"#,
    );
    assert!(matches!(
        session.stories.post.outcome,
        StoryPostOutcome::Failed(_)
    ));

    // A raw `error` answer on `postStory` → Failed; on `canPostStory`
    // → check_error (the composer stops spinning either way).
    let extra = session.request(RequestPurpose::PostStory, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"x"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.stories.post.outcome,
        StoryPostOutcome::Failed(_)
    ));
    let extra = session.request(RequestPurpose::CheckCanPostStory, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":401,"message":"x"}}"#,
            extra.0
        ),
    );
    assert!(session.stories.post.check_error.is_some());
}

#[test]
fn story_manage_state_transitions() {
    // Phase 9.5: `editStory` / `editStoryCover` /
    // `setStoryPrivacySettings` pending is cleared by the `ok`
    // answer and the sanitized error lands on failure;
    // `getChatsToPostStories` stores the chat ids.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);

    let extra = session.request(RequestPurpose::EditStory, None);
    session.stories.manage.pending = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.stories.manage.pending);
    assert_eq!(session.stories.manage.error, None);

    let extra = session.request(RequestPurpose::EditStoryCover, None);
    session.stories.manage.pending = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"STORY_NOT_EDITABLE"}}"#,
            extra.0
        ),
    );
    assert!(!session.stories.manage.pending);
    let error = session.stories.manage.error.clone().expect("manage error");
    assert!(error.contains("Story update failed"), "{error}");

    let extra = session.request(RequestPurpose::GetChatsToPostStories, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[111,222]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.stories.post_as_chats, vec![111, 222]);

    // Review fix-up: a failed `getChatsToPostStories` surfaces a
    // transient error instead of silently leaving only "Myself".
    let extra = session.request(RequestPurpose::GetChatsToPostStories, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"SOME_ERROR"}}"#,
            extra.0
        ),
    );
    let error = session
        .stories
        .post
        .check_error
        .clone()
        .expect("post-as error");
    assert!(error.contains("Could not load"), "{error}");
}

#[test]
fn story_post_second_answer_without_pending_is_absorbed() {
    // Phase 9.3 (review fix-up): the double-post window. The UI
    // `post_sent` guard blocks the second send path, and at the
    // reducer level a `postStory` answer with no matching pending
    // `PostStory` request is absorbed as a plain story upsert — it
    // neither re-enters `Posting` nor creates a second pending
    // request.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);

    // The (single) send: pending `PostStory`, answer → Posting.
    let extra = session.request(RequestPurpose::PostStory, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"story","@extra":"{}","id":8,"poster_chat_id":777,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.stories.post.outcome,
        StoryPostOutcome::Posting { story_id: 8 }
    );
    assert!(!session.requests.has_purpose(RequestPurpose::PostStory));

    // A second `story` answer with no pending `PostStory` request
    // (the duplicate the guard prevents) is just a story upsert.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"story","id":8,"poster_chat_id":777,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}}"#,
    );
    assert_eq!(
        session.stories.post.outcome,
        StoryPostOutcome::Posting { story_id: 8 }
    );
    assert!(!session.requests.has_purpose(RequestPurpose::PostStory));
    assert!(session.stories.stories.contains_key(&(777, 8)));
}

#[test]
fn get_story_response_lands_in_story_cache() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_story(RequestPurpose::GetStory, ChatId(11), 5);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"story","@extra":"{}","id":5,"poster_chat_id":11,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"CANARY_STORY","entities":[]}}}}"#,
            extra.0
        ),
    );
    let story = session.stories.stories.get(&(11, 5)).expect("story cached");
    assert_eq!(story.caption, "CANARY_STORY");
    assert!(matches!(
        story.content,
        crate::telegram::envelope::StoryContentView::Unsupported
    ));
}

#[test]
fn get_story_dedupes_in_flight_per_story() {
    let (mut session, _sink) = session();
    let extra1 = session.request_for_story(RequestPurpose::GetStory, ChatId(11), 5);
    assert!(
        session
            .requests
            .has_purpose_for_story(RequestPurpose::GetStory, ChatId(11), 5)
    );
    // Same chat, different story → not suppressed.
    assert!(
        !session
            .requests
            .has_purpose_for_story(RequestPurpose::GetStory, ChatId(11), 6)
    );
    session.requests.take(extra1);
    assert!(
        !session
            .requests
            .has_purpose_for_story(RequestPurpose::GetStory, ChatId(11), 5)
    );
}

#[test]
fn call_history_pages_accumulate_and_track_offset() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::SearchCallMessages, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":2,"next_offset":"page2","messages":[{{"@type":"message","id":901,"chat_id":71,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageCall","unique_id":901,"is_video":true,"discard_reason":{{"@type":"callDiscardReasonHungUp"}},"duration":372}}}}]}}"#,
            extra.0,
        ),
    );
    assert_eq!(session.calls.recent_calls.len(), 1);
    assert_eq!(session.calls.recent_calls_offset, "page2");
    assert!(!session.calls.recent_calls_loading);
    assert!(!session.calls.recent_calls_error);
    let extra = session.request(RequestPurpose::SearchCallMessages, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":2,"next_offset":"","messages":[{{"@type":"message","id":900,"chat_id":71,"is_outgoing":false,"date":1699999999,"content":{{"@type":"messageCall","unique_id":900,"is_video":false,"discard_reason":{{"@type":"callDiscardReasonMissed"}},"duration":0}}}}]}}"#,
            extra.0,
        ),
    );
    assert_eq!(session.calls.recent_calls.len(), 2);
    assert_eq!(session.calls.recent_calls_offset, "");
    assert_eq!(session.calls.recent_calls[0].id.0, 901);
    assert_eq!(session.calls.recent_calls[1].id.0, 900);
}

#[test]
fn set_account_ttl_ok_stores_confirmed_days() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.account_ttl_days = Some(90);
    session.settings.account_mutating = true;
    let extra = session.request(
        RequestPurpose::Settings(SettingsPurpose::SetAccountTtl { days: 365 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.settings.account_ttl_days, Some(365));
    assert!(!session.settings.account_mutating);
    assert!(session.settings.account_error.is_none());
}

#[test]
fn code_info_answer_stores_pending_number_for_matching_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.auth_state.change_number_loading = true;
    session.auth_state.change_number_error = Some("stale".into());
    let extra = session.request(RequestPurpose::SendPhoneNumberCode, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"authenticationCodeInfo","@extra":"{}","phone_number":"+15550199","type":{{"@type":"authenticationCodeTypeSms","length":5}},"next_type":null,"timeout":60}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.auth_state.change_number_phone.as_deref(),
        Some("+15550199")
    );
    assert_eq!(session.auth_state.change_number_timeout, Some(60));
    assert!(!session.auth_state.change_number_loading);
    assert!(session.auth_state.change_number_error.is_none());
}

#[test]
fn per_message_updates_reach_loaded_topic_histories() {
    // The topic view reads `topic_histories` only, so every per-message
    // update must reach its copy of the row, not just the main history.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(16));
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"foundChatMessages\",\"@extra\":\"{}\",{}}}",
            extra.0,
            r#""total_count":2,"next_from_message_id":0,"messages":[{"id":50,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"before","entities":[]}}},{"id":60,"chat_id":16,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":1,"vote_percentage":100,"is_chosen":false}],"total_voter_count":1,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}]"#
        ),
    );
    for update in [
        r#"{"@type":"updateMessageContent","chat_id":16,"message_id":50,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"after","entities":[]}}}"#,
        r#"{"@type":"updateMessageIsPinned","chat_id":16,"message_id":50,"is_pinned":true}"#,
        r#"{"@type":"updateMessageInteractionInfo","chat_id":16,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":7,"forward_count":0,"reply_info":null,"reactions":null}}"#,
        r#"{"@type":"updateMessageEdited","chat_id":16,"message_id":50,"edit_date":1700000001,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"New","icon_custom_emoji_id":0,"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AA=="}}]]}}"#,
        r#"{"@type":"updatePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":2,"vote_percentage":100,"is_chosen":true}],"total_voter_count":2,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}}}"#,
    ] {
        apply_json(&mut session, &seq, &sink, update);
    }
    let topic = &session.threads.topic_histories[&(16, 2)];
    let row = &topic.messages[&50];
    assert!(
        matches!(&row.content, MessageContent::Text(text) if text.text == "after"),
        "{:?}",
        row.content
    );
    assert!(row.is_pinned);
    assert_eq!(row.interaction_info.as_ref().map(|i| i.view_count), Some(7));
    assert!(row.reply_markup.is_some());
    let MessageContent::Poll(poll) = &topic.messages[&60].content else {
        panic!("poll row");
    };
    assert_eq!(poll.poll.total_voter_count, 2);
    assert!(poll.poll.options[0].is_chosen);
}

#[test]
fn b14_close_friends_load_and_save_round_trip() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // A stray `users` answer never becomes the close-friends list.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"users","@extra":"no-such","total_count":1,"user_ids":[9]}"#,
    );
    assert!(session.stories.close_friends.is_none());

    let extra = session.request(RequestPurpose::GetCloseFriends, None);
    session.begin_story_page_check(crate::story_page::story_page_op_label(
        RequestPurpose::GetCloseFriends,
    ));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"users","@extra":"{}","total_count":2,"user_ids":[31,33]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.stories.close_friends, Some(vec![31, 33]));
    assert!(session.stories.page_op.is_none());

    // `setCloseFriends` applies the staged ids on `ok`.
    session.stories.close_friends_pending = Some(vec![33]);
    let extra = session.request(RequestPurpose::SetCloseFriends, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.stories.close_friends, Some(vec![33]));
    assert!(session.stories.close_friends_pending.is_none());
}

#[test]
fn b14_close_friends_error_drops_staged_ids() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.stories.close_friends = Some(vec![31]);
    session.stories.close_friends_pending = Some(vec![31, 32]);
    session.begin_story_page_op(crate::story_page::story_page_op_label(
        RequestPurpose::SetCloseFriends,
    ));
    let extra = session.request(RequestPurpose::SetCloseFriends, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"USER_NOT_MUTUAL_CONTACT"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.stories.close_friends, Some(vec![31]));
    assert!(session.stories.close_friends_pending.is_none());
    assert!(matches!(
        session.stories.page_op.as_ref().map(|op| &op.state),
        Some(crate::story_page::StoryPageOpState::Failed(_))
    ));
}

#[test]
fn b14_hide_and_profile_ops_finish_on_ok() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for purpose in [
        RequestPurpose::SetChatActiveStoriesList,
        RequestPurpose::ToggleStoryIsPostedToChatPage,
    ] {
        session.begin_story_page_op(crate::story_page::story_page_op_label(purpose));
        let extra = session.request(purpose, Some(ChatId(11)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert_eq!(
            session.stories.page_op.as_ref().map(|op| &op.state),
            Some(&crate::story_page::StoryPageOpState::Succeeded)
        );
    }
}

#[test]
fn b14_story_parses_profile_and_statistics_flags() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateStory","story":{"@type":"story","id":5,"poster_chat_id":11,"date":1,"is_posted_to_chat_page":true,"can_toggle_is_posted_to_chat_page":true,"can_get_statistics":true,"can_be_forwarded":true,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}}}"#,
    );
    let story = session.stories.stories.get(&(11, 5)).expect("story cached");
    assert!(story.is_posted_to_chat_page);
    assert!(story.can_toggle_is_posted_to_chat_page);
    assert!(story.can_get_statistics);
    assert!(crate::story_extras::can_share_story(story));
    assert!(crate::story_extras::can_save_story(story));
}

#[test]
fn story_custom_emoji_stickers_are_bounded() {
    use crate::telegram::envelope::{StickerFormat, StickerItem};
    let (mut session, _) = session();
    let sticker = |id: i64| StickerItem {
        custom_emoji_id: Some(id),
        id,
        set_id: 1,
        emoji: String::new(),
        width: 64,
        height: 64,
        format: StickerFormat::Webp,
        file_id: crate::ids::FileId(1),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
    };
    let total = crate::state::STORY_CUSTOM_EMOJI_CAP as i64 * 3;
    for id in 1..=total {
        session.accept_story_custom_emoji_stickers(vec![sticker(id)]);
        assert!(
            session.stories.custom_emoji_stickers.len() <= crate::state::STORY_CUSTOM_EMOJI_CAP
        );
    }
    assert!(session.stories.custom_emoji_stickers.contains_key(&total));
}
