//! Connect-driver tests: a `getStory` that failed for a reason that may pass
//! is not asked again on every viewer tick, but a user opening the story
//! asks at once.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{STORIES_RETRY_FIRST, STORIES_RETRY_MAX, Session, StoryCardState};
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Harness {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    dir: std::path::PathBuf,
}

impl Harness {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &sink);
        Harness {
            driver,
            recorder,
            seq,
            sink,
            dir,
        }
    }

    fn fail(&mut self, extra: RequestId, code: i32) {
        let json = format!(
            r#"{{"@type":"error","@extra":"{}","code":{code},"message":"x"}}"#,
            extra.0
        );
        self.driver
            .ingest(copy_and_parse(&json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn get_story_sends(&self) -> usize {
        self.recorder
            .snapshot()
            .iter()
            .filter(|json| json.contains(r#""@type":"getStory""#))
            .count()
    }
}

impl Harness {
    /// The open chat shows a card for story `(7, story)`.
    fn show_story_card(&mut self, story: i32) {
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":21,"is_outgoing":false,"date":1,"content":{{"@type":"messageStory","story_poster_chat_id":7,"story_id":{story},"via_mention":false}}}}}}"#,
            400 + story
        );
        self.driver
            .ingest(copy_and_parse(&json, &self.seq, &self.sink).unwrap())
            .unwrap();
        self.driver.session.open_chat = Some(ChatId(21));
    }

    /// The `@extra` of the last `getStory` sent.
    fn last_get_story(&self) -> RequestId {
        self.recorder
            .snapshot()
            .iter()
            .rev()
            .filter_map(|json| serde_json::from_str::<serde_json::Value>(json).ok())
            .find(|v| v["@type"] == "getStory")
            .and_then(|v| v["@extra"].as_str()?.parse().ok())
            .map(RequestId)
            .expect("a getStory")
    }

    /// The wait after a failure for `(7, story)` is over.
    fn end_wait(&mut self, story: i32) {
        if let Some(entry) = self.driver.session.stories.story_retry.get_mut(&(7, story)) {
            entry.0 = std::time::Instant::now();
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_500_is_not_asked_again_on_every_frame() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .get_story(ChatId(7), 5)
        .unwrap()
        .expect("getStory sends");
    h.fail(extra, 500);
    assert_eq!(
        h.driver.session.story_card_state(7, 5),
        StoryCardState::Loading,
        "a 500 does not mean the story is gone"
    );
    for _ in 0..5 {
        assert_eq!(
            h.driver.get_story(ChatId(7), 5),
            Ok(None),
            "a prefetch waits"
        );
    }
    assert_eq!(h.get_story_sends(), 1);
    assert_eq!(
        h.driver
            .get_story(ChatId(7), 6)
            .map(|extra| extra.is_some()),
        Ok(true),
        "other stories are not held back"
    );
}

#[test]
fn opening_the_story_after_a_failure_asks_again_at_once() {
    let mut h = Harness::new();
    let extra = h.driver.get_story(ChatId(7), 5).unwrap().expect("sends");
    h.fail(extra, 429);
    let extra = h
        .driver
        .get_story_now(ChatId(7), 5)
        .unwrap()
        .expect("the open asks again");
    assert_eq!(h.get_story_sends(), 2);
    h.fail(extra, 500);
    let (_, wait) = h
        .driver
        .session
        .stories
        .story_retry
        .get(&(7, 5))
        .copied()
        .expect("waiting again");
    assert_eq!(wait, STORIES_RETRY_FIRST, "the open started a new clock");
    assert_eq!(h.driver.get_story(ChatId(7), 5), Ok(None));
    assert_eq!(h.get_story_sends(), 2);
}

#[test]
fn each_failure_doubles_the_wait_up_to_the_cap() {
    let mut h = Harness::new();
    let stories = &mut h.driver.session.stories;
    let now = std::time::Instant::now();
    stories.story_fetch_failed((7, 5), now);
    stories.story_fetch_failed((7, 5), now);
    assert_eq!(
        stories.story_retry.get(&(7, 5)).map(|(_, wait)| *wait),
        Some(STORIES_RETRY_FIRST * 2)
    );
    for _ in 0..20 {
        stories.story_fetch_failed((7, 5), now);
    }
    assert_eq!(
        stories.story_retry.get(&(7, 5)).map(|(_, wait)| *wait),
        Some(STORIES_RETRY_MAX)
    );
    assert!(stories.story_fetch_waiting((7, 5), now));
    assert!(!stories.story_fetch_waiting((7, 5), now + STORIES_RETRY_MAX));
}

#[test]
fn the_story_arriving_ends_the_wait() {
    let mut h = Harness::new();
    let extra = h.driver.get_story(ChatId(7), 5).unwrap().expect("sends");
    h.fail(extra, 500);
    seed_story(
        &mut h.driver,
        &h.seq,
        &h.sink,
        7,
        5,
        "storyContentPhoto",
        "",
    );
    assert!(h.driver.session.stories.story_retry.is_empty());
}

/// Regression: the card counted as asked before `getStory` ran, so a 500
/// left it on Loading for the session.
#[test]
fn a_card_whose_story_failed_asks_again_after_the_wait() {
    let mut h = Harness::new();
    h.show_story_card(5);
    h.driver.maybe_fetch_replied_messages().unwrap();
    h.driver.maybe_fetch_replied_messages().unwrap();
    assert_eq!(h.get_story_sends(), 1, "asked once");
    let extra = h.last_get_story();
    h.fail(extra, 500);
    h.driver.maybe_fetch_replied_messages().unwrap();
    assert_eq!(h.get_story_sends(), 1, "not while waiting");
    h.end_wait(5);
    h.driver.maybe_fetch_replied_messages().unwrap();
    assert_eq!(h.get_story_sends(), 2, "asked again after the wait");
    let extra = h.last_get_story();
    h.fail(extra, 404);
    h.driver.maybe_fetch_replied_messages().unwrap();
    assert_eq!(h.get_story_sends(), 2, "a gone story is not asked again");
}

/// Regression: a card whose story a viewer prefetch failed for was marked
/// asked while the wait held its `getStory` back, so it never asked.
#[test]
fn a_card_behind_a_failed_prefetch_asks_once_the_wait_is_over() {
    let mut h = Harness::new();
    let extra = h.driver.get_story(ChatId(7), 6).unwrap().expect("sends");
    h.fail(extra, 500);
    h.show_story_card(6);
    h.driver.maybe_fetch_replied_messages().unwrap();
    assert_eq!(h.get_story_sends(), 1);
    h.end_wait(6);
    h.driver.maybe_fetch_replied_messages().unwrap();
    assert_eq!(h.get_story_sends(), 2);
}

#[test]
fn coming_back_online_ends_every_story_wait() {
    let mut h = Harness::new();
    let connection = |h: &mut Harness, state: &str| {
        let json = format!(r#"{{"@type":"updateConnectionState","state":{{"@type":"{state}"}}}}"#);
        h.driver
            .ingest(copy_and_parse(&json, &h.seq, &h.sink).unwrap())
            .unwrap();
    };
    connection(&mut h, "connectionStateReady");
    let extra = h.driver.get_story(ChatId(7), 5).unwrap().expect("sends");
    h.fail(extra, 500);
    connection(&mut h, "connectionStateReady");
    assert_eq!(h.driver.get_story(ChatId(7), 5), Ok(None), "still online");
    connection(&mut h, "connectionStateConnecting");
    assert_eq!(h.driver.get_story(ChatId(7), 5), Ok(None));
    connection(&mut h, "connectionStateReady");
    assert!(
        h.driver.get_story(ChatId(7), 5).unwrap().is_some(),
        "asked at once after the reconnect"
    );
}
