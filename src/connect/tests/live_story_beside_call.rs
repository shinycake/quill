//! Connect-driver tests: a live story plays in its own receive-only slot
//! beside a 1:1 call or a video chat (tdesktop `registerVideoStream`), and
//! is never the current call.
use super::live_story::{Harness, LIVE_CALL, harness, watching};
use super::*;
use crate::calls::engine::MockEngine;
use crate::calls::group_call_create::CurrentCall;
use crate::ids::ChatId;

const STORY: &str = r#"{"@type":"story","id":6,"poster_chat_id":7,"date":1700000000,"content":{"@type":"storyContentLive","group_call_id":4242,"is_rtmp_stream":false},"caption":{"@type":"formattedText","text":"","entities":[]}}"#;

const ONE_TO_ONE: &str = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#;

const ONE_TO_ONE_ENDED: &str = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#;

const VIDEO_CHAT: &str = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"title":"Team","is_active":ACTIVE,"is_video_chat":true,"is_joined":JOINED}}"#;

fn video_chat(active: bool, joined: bool) -> String {
    VIDEO_CHAT
        .replace("ACTIVE", &active.to_string())
        .replace("JOINED", &joined.to_string())
}

fn add_team_chat(h: &mut Harness) {
    h.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"Team","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0}}"#,
    );
    h.ingest(
        r#"{"@type":"updateChatVideoChat","chat_id":-100,"video_chat":{"@type":"videoChat","group_call_id":555,"has_participants":true}}"#,
    );
}

fn open_story(h: &mut Harness) {
    h.ingest(STORY);
    h.driver
        .join_live_story(ChatId(7), 6)
        .expect("the story joins beside the other call");
    h.ingest(&LIVE_CALL.replace("JOINED", "false"));
}

fn story_id(h: &Harness) -> Option<i32> {
    h.driver
        .session
        .calls
        .story_stream
        .as_ref()
        .map(|call| call.id)
}

fn current_id(h: &Harness) -> Option<i32> {
    h.driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .map(|call| call.id)
}

#[test]
fn a_live_story_opens_beside_a_one_to_one_call() {
    let mut h = harness();
    h.ingest(ONE_TO_ONE);
    open_story(&mut h);
    assert_eq!(story_id(&h), Some(4242), "the story has its own slot");
    assert_eq!(h.sent("joinLiveStory"), 1, "joined without asking");
    assert!(
        h.driver.session.calls.active_call.is_some(),
        "the call stays"
    );
    assert_eq!(current_id(&h), None, "the story is not the current call");
}

#[test]
fn a_live_story_opens_beside_a_joined_video_chat() {
    let mut h = harness();
    add_team_chat(&mut h);
    h.ingest(&video_chat(true, true));
    open_story(&mut h);
    assert_eq!(story_id(&h), Some(4242));
    assert_eq!(current_id(&h), Some(555), "the video chat keeps its slot");
    assert_eq!(h.sent("joinLiveStory"), 1);
    assert_eq!(h.sent("leaveGroupCall"), 0, "nothing is left for it");
}

#[test]
fn leaving_the_story_leaves_only_its_slot() {
    let mut h = harness();
    let engine = MockEngine::new();
    let handle = engine.clone();
    h.driver.set_call_engine(Box::new(engine));
    add_team_chat(&mut h);
    h.ingest(&video_chat(true, true));
    open_story(&mut h);
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver.leave_live_story().expect("the story is left");
    assert_eq!(story_id(&h), None);
    assert_eq!(current_id(&h), Some(555), "the video chat stays");
    assert_eq!(
        sent_request(&h.recorder, "leaveGroupCall")["group_call_id"],
        4242
    );
    assert_eq!(h.sent("leaveGroupCall"), 1);
    assert_eq!(
        handle.group_leaves(),
        vec![4242],
        "only its native call stops"
    );
    assert_eq!(h.driver.session.calls.left_group_call, None);
}

#[test]
fn ending_the_call_does_not_end_the_story() {
    let mut h = harness();
    add_team_chat(&mut h);
    h.ingest(ONE_TO_ONE);
    open_story(&mut h);
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.ingest(ONE_TO_ONE_ENDED);
    assert!(h.driver.session.calls.active_call.is_none());
    assert_eq!(story_id(&h), Some(4242), "the 1:1 call's end leaves it");
    h.ingest(&video_chat(true, true));
    h.ingest(&video_chat(false, false));
    assert_eq!(current_id(&h), None);
    assert_eq!(story_id(&h), Some(4242), "the video chat's end leaves it");
    assert!(h.driver.session.stories.live.is_some());
}

#[test]
fn the_story_is_not_the_current_call() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    assert_eq!(
        h.driver.session.current_call_for_create(None),
        CurrentCall::None
    );
    assert!(!h.driver.session.group_call_joined_or_joining(None));
}

#[test]
fn a_video_chat_joins_beside_an_open_story() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    add_team_chat(&mut h);
    h.ingest(&video_chat(true, false));
    assert_eq!(current_id(&h), Some(555));
    h.driver
        .join_video_chat(555)
        .expect("joined without asking");
    assert_eq!(
        sent_request(&h.recorder, "joinVideoChat")["group_call_id"],
        555
    );
    assert_eq!(story_id(&h), Some(4242), "the story slot is left alone");
    assert_eq!(h.sent("leaveGroupCall"), 0);
}

#[test]
fn a_real_call_still_asks_with_a_story_open() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.ingest(ONE_TO_ONE);
    assert_eq!(
        h.driver.session.current_call_for_create(None),
        CurrentCall::OneToOne
    );
    h.ingest(ONE_TO_ONE_ENDED);
    add_team_chat(&mut h);
    h.ingest(&video_chat(true, true));
    assert!(matches!(
        h.driver.session.current_call_for_create(None),
        CurrentCall::Group { .. }
    ));
    assert_eq!(story_id(&h), Some(4242));
}

#[test]
fn a_late_join_answer_does_not_land_in_a_new_story_slot() {
    let mut h = watching();
    let old = sent_request(&h.recorder, "joinLiveStory")["@extra"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_default();
    h.driver.leave_live_story().unwrap();
    h.driver.join_live_story(ChatId(7), 6).unwrap();
    h.ingest(&LIVE_CALL.replace("JOINED", "false"));
    assert_eq!(h.sent("joinLiveStory"), 2);
    h.ingest(&format!(
        r#"{{"@type":"text","@extra":"{old}","text":"{{\"old\":true}}"}}"#
    ));
    let payload = h
        .driver
        .session
        .calls
        .story_stream
        .as_ref()
        .map(|call| call.join_payload.clone());
    assert_eq!(payload, Some(String::new()), "the old answer is dropped");
}

#[test]
fn a_failed_story_leave_does_not_reach_the_call_overlay() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver.leave_live_story().unwrap();
    h.answer(
        "leaveGroupCall",
        r#"{"@type":"error","code":400,"message":"GROUPCALL_JOIN_MISSING"}"#,
    );
    assert!(h.driver.session.calls.group_call_error.is_none());
}

/// A 1:1 call with the story's poster (user 7, whose private chat is 7).
const POSTER_CALL: &str = r#"{"@type":"updateCall","call":{"@type":"call","id":78,"unique_id":"98","user_id":7,"is_outgoing":true,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#;

fn with_engine(h: &mut Harness) -> MockEngine {
    let mock = MockEngine::new();
    let handle = mock.clone();
    h.driver.set_call_engine(Box::new(mock));
    handle
}

fn story_failed(h: &Harness) -> bool {
    h.driver
        .session
        .calls
        .story_stream
        .as_ref()
        .is_some_and(|call| call.join_failed())
}

#[test]
fn a_story_opened_during_a_call_with_its_poster_plays_beside_it() {
    let mut h = harness();
    let handle = with_engine(&mut h);
    h.ingest(POSTER_CALL);
    assert_eq!(handle.started_calls().len(), 1);
    open_story(&mut h);
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    assert!(!story_failed(&h));
    let call = h.driver.session.calls.active_call.as_ref();
    assert!(call.is_some_and(|call| call.transport_error.is_none()));
    assert!(handle.group_leaves().is_empty());
}

#[test]
fn a_call_with_the_poster_leaves_the_story_playing() {
    let mut h = harness();
    let handle = with_engine(&mut h);
    open_story(&mut h);
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.ingest(POSTER_CALL);
    assert!(
        handle.group_leaves().is_empty(),
        "the story's transport stays"
    );
    assert!(!story_failed(&h));
    assert_eq!(handle.started_calls().len(), 1);
    let call = h.driver.session.calls.active_call.as_ref();
    assert!(call.is_some_and(|call| call.transport_error.is_none()));
}

#[test]
fn a_failed_first_join_shows_on_the_story() {
    let mut h = watching();
    h.answer(
        "joinLiveStory",
        r#"{"@type":"error","code":400,"message":"GROUPCALL_INVALID"}"#,
    );
    assert!(story_failed(&h));
    assert!(h.driver.session.calls.group_call_error.is_none());
}

#[test]
fn spent_rejoins_show_on_the_story() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    if let Some(call) = h.driver.session.calls.story_stream.as_mut() {
        call.reconnecting = true;
        call.rejoin_attempts = crate::state::MAX_GROUP_CALL_REJOIN_ATTEMPTS;
    }
    assert!(story_failed(&h));
}

/// A live story posted by the Team chat (-100), whose voice chat is 555.
const TEAM_STORY: &str = r#"{"@type":"story","id":6,"poster_chat_id":-100,"date":1700000000,"content":{"@type":"storyContentLive","group_call_id":4242,"is_rtmp_stream":false},"caption":{"@type":"formattedText","text":"","entities":[]}}"#;

fn open_team_story(h: &mut Harness) {
    h.ingest(TEAM_STORY);
    h.driver
        .join_live_story(ChatId(-100), 6)
        .expect("the story is looked up");
    h.ingest(&LIVE_CALL.replace("JOINED", "false"));
}

/// Stream pictures, mode and quality are keyed by chat: the same chat's
/// story and call would share them, so the call you join wins.
#[test]
fn joining_the_story_chats_call_stops_the_story() {
    let mut h = harness();
    let handle = with_engine(&mut h);
    add_team_chat(&mut h);
    open_team_story(&mut h);
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    assert_eq!(h.sent("joinLiveStory"), 1);
    assert!(!story_failed(&h));
    h.ingest(&video_chat(true, false));
    h.driver.join_video_chat(555).expect("the call joins");
    assert_eq!(handle.group_leaves(), vec![4242], "the story's media goes");
    assert!(story_failed(&h), "the viewer shows it failed, with Retry");
    assert_eq!(
        handle.group_offers().last(),
        Some(&(555, -100)),
        "the call gets the chat's stream"
    );
    assert_eq!(h.sent("joinVideoChat"), 1);
}

#[test]
fn a_story_of_the_chat_youre_in_a_call_with_does_not_join() {
    let mut h = harness();
    let handle = with_engine(&mut h);
    add_team_chat(&mut h);
    h.ingest(&video_chat(true, false));
    h.driver.join_video_chat(555).expect("the call joins");
    h.ingest(&video_chat(true, true));
    open_team_story(&mut h);
    assert_eq!(h.sent("joinLiveStory"), 0, "no join goes out");
    assert!(story_failed(&h));
    assert!(
        handle.group_offers().iter().all(|(id, _)| *id != 4242),
        "no engine instance for the story"
    );
    assert!(handle.group_leaves().is_empty(), "the call stays");
    assert_eq!(current_id(&h), Some(555));
}

#[test]
fn a_story_of_another_chat_keeps_playing_beside_the_call() {
    let mut h = harness();
    let handle = with_engine(&mut h);
    add_team_chat(&mut h);
    open_story(&mut h);
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.ingest(&video_chat(true, false));
    h.driver.join_video_chat(555).expect("the call joins");
    assert!(handle.group_leaves().is_empty());
    assert!(!story_failed(&h));
}
