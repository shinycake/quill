//! Connect-driver tests: story lists page to their 404, because TDLib sends
//! no `updateChatActiveStories` for chats past the point loaded.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::StoryListView;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

struct Lists {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    seen: usize,
}

impl Lists {
    fn ready() -> Self {
        let store = MemorySecretStore::new();
        let (_dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut lists = Self {
            driver,
            recorder,
            sink,
            seq: AtomicU64::new(0),
            seen: 0,
        };
        lists.ingest(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        lists
    }

    fn ingest(&mut self, json: &str) {
        let envelope = copy_and_parse(json, &self.seq, &self.sink).unwrap();
        self.driver.ingest(envelope).unwrap();
    }

    /// New `loadActiveStories` sends as `(story list, @extra)`.
    fn pages(&mut self) -> Vec<(String, String)> {
        let sent = self.recorder.snapshot();
        let fresh = sent.get(self.seen..).unwrap_or_default().to_vec();
        self.seen = sent.len();
        fresh
            .iter()
            .filter_map(|json| serde_json::from_str::<Value>(json).ok())
            .filter(|v| v["@type"] == "loadActiveStories")
            .map(|v| {
                (
                    v["story_list"]["@type"].as_str().unwrap().to_owned(),
                    v["@extra"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    fn answer(&mut self, extra: &str, body: &str) {
        self.ingest(&format!(r#"{{{body},"@extra":"{extra}"}}"#));
    }
}

const OK: &str = r#""@type":"ok""#;
const NOT_FOUND: &str = r#""@type":"error","code":404,"message":"Not Found""#;
const FAILED: &str = r#""@type":"error","code":500,"message":"x""#;

#[test]
fn both_lists_page_to_their_404_after_ready() {
    let mut lists = Lists::ready();
    let first = lists.pages();
    let names: Vec<&str> = first.iter().map(|(list, _)| list.as_str()).collect();
    assert_eq!(names, ["storyListMain", "storyListArchive"]);
    let (main, hidden) = (first[0].1.clone(), first[1].1.clone());

    lists.answer(&main, OK);
    let next = lists.pages();
    assert_eq!(next.len(), 1, "one more page: {next:?}");
    assert_eq!(next[0].0, "storyListMain");

    lists.answer(&next[0].1.clone(), NOT_FOUND);
    assert_eq!(lists.pages(), [], "the main list is done");

    lists.answer(&hidden, OK);
    let next = lists.pages();
    assert_eq!(next[0].0, "storyListArchive");
    lists.answer(&next[0].1.clone(), NOT_FOUND);
    assert_eq!(lists.pages(), []);
}

#[test]
fn a_failed_page_resumes_on_the_tick_after_its_wait() {
    let mut lists = Lists::ready();
    let first = lists.pages();
    let main = first[0].1.clone();
    lists.answer(&main, FAILED);
    assert_eq!(lists.pages(), [], "no page while waiting");
    assert!(!lists.driver.active_stories_tick(Instant::now()));
    let (at, wait) = lists.driver.session.stories.main_paging.retry().unwrap();
    assert!(wait > Duration::ZERO);
    assert!(
        lists
            .driver
            .session
            .stories
            .paging(StoryListView::Main)
            .may_load(at)
    );
    // The wait is measured from the failure, so a tick past it sends.
    lists.driver.session.stories.main_paging = crate::state::ActiveStoriesPaging::Waiting {
        until: Instant::now(),
        wait,
    };
    assert!(lists.driver.active_stories_tick(Instant::now()));
    assert_eq!(lists.pages()[0].0, "storyListMain");
    assert!(
        !lists.driver.active_stories_tick(Instant::now()),
        "one page at a time"
    );
}
