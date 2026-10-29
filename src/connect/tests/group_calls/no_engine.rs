//! Connect-driver tests: without a call engine (libquill_calls did not
//! load, or loaded unavailable) no voice chat, live stream or live story
//! is joined, since nothing would play or send. The join is refused with
//! `CallsUnavailable` before anything goes to TDLib, so the UI can say
//! calls are unavailable, as it does for a 1:1 call.
use super::*;
use crate::ids::ChatId;
use crate::state::ActiveGroupCall;
use std::sync::atomic::AtomicU64;

static SEQ: AtomicU64 = AtomicU64::new(60_000);

const LIVE_STORY: &str = r#"{"@type":"story","id":6,"poster_chat_id":7,"date":1700000000,"content":{"@type":"storyContentLive","group_call_id":4242,"is_rtmp_stream":false},"caption":{"@type":"formattedText","text":"","entities":[]}}"#;

fn ingest(driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    driver
        .ingest(copy_and_parse(json, &SEQ, &sink).unwrap())
        .unwrap();
}

fn sent(recorder: &RecordingSender, method: &str) -> usize {
    let needle = format!("\"@type\":\"{method}\"");
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(&needle))
        .count()
}

/// A driver tracking voice chat 555, unjoined, as `getGroupCall` leaves it.
fn tracking_a_voice_chat() -> (
    std::path::PathBuf,
    Arc<RecordingSender>,
    ConnectDriver<Arc<RecordingSender>>,
) {
    let (dir, recorder, mut driver, _seq) = group_call_test_driver();
    driver.session.calls.active_group_call = Some(ActiveGroupCall::fresh(555));
    (dir, recorder, driver)
}

#[test]
fn a_voice_chat_is_not_joined_without_an_engine() {
    let (dir, recorder, mut driver) = tracking_a_voice_chat();
    let pending = driver.session.requests.pending.len();
    assert_eq!(
        driver.join_video_chat(555),
        Err(ConnectSendError::CallsUnavailable)
    );
    assert_eq!(sent(&recorder, "joinVideoChat"), 0, "nothing goes out");
    assert_eq!(driver.session.requests.pending.len(), pending);
    assert!(!driver.session.group_call_joined_or_joining(None));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unavailable_engine_counts_as_none() {
    let (dir, recorder, mut driver) = tracking_a_voice_chat();
    let engine = MockEngine::unavailable();
    let handle = engine.clone();
    driver.set_call_engine(Box::new(engine));
    assert_eq!(
        driver.join_video_chat(555),
        Err(ConnectSendError::CallsUnavailable)
    );
    assert_eq!(sent(&recorder, "joinVideoChat"), 0);
    assert!(handle.group_offers().is_empty(), "no media is set up");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_live_stream_is_not_rejoined_without_an_engine() {
    let (dir, recorder, mut driver) = tracking_a_voice_chat();
    if let Some(call) = driver.session.calls.active_group_call.as_mut() {
        call.reconnecting = true;
    }
    assert_eq!(
        driver.rejoin_group_call(true),
        Err(ConnectSendError::CallsUnavailable)
    );
    driver.maybe_auto_rejoin_group_call().unwrap();
    assert_eq!(sent(&recorder, "joinVideoChat"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_live_story_is_not_joined_without_an_engine() {
    let (dir, recorder, mut driver, _seq) = group_call_test_driver();
    ingest(&mut driver, LIVE_STORY);
    assert_eq!(
        driver.join_live_story(ChatId(7), 6),
        Err(ConnectSendError::CallsUnavailable)
    );
    assert_eq!(sent(&recorder, "getGroupCall"), 0, "not even looked up");
    assert_eq!(driver.session.calls.pending_live_story_join, None);
    assert!(driver.session.stories.live.is_none());
    assert!(driver.session.calls.story_stream.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_an_engine_the_same_joins_go_out() {
    let (dir, recorder, mut driver) = tracking_a_voice_chat();
    driver.set_call_engine(Box::new(MockEngine::new()));
    ingest(&mut driver, LIVE_STORY);
    driver.join_live_story(ChatId(7), 6).expect("story joins");
    assert_eq!(sent(&recorder, "getGroupCall"), 1);
    driver.join_video_chat(555).expect("voice chat joins");
    assert_eq!(sent(&recorder, "joinVideoChat"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
