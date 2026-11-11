//! Connect-driver tests: story statistics, public forwards, public search.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::{Session, StorySearchQuery};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn sent_types(recorder: &RecordingSender) -> Vec<Value> {
    recorder
        .snapshot()
        .into_iter()
        .map(|json| serde_json::from_str::<Value>(&json).unwrap())
        .collect()
}

#[test]
fn story_insights_send_both_requests_only_when_allowed() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Unknown story, then one without `can_get_statistics`: refused.
    assert_eq!(
        driver.fetch_story_insights(ChatId(7), 5, false),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
    assert_eq!(
        driver.fetch_story_insights(ChatId(7), 5, false),
        Err(ConnectSendError::InvalidRequest)
    );

    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        6,
        "storyContentPhoto",
        r#""can_get_statistics":true,"#,
    );
    driver
        .fetch_story_insights(ChatId(7), 6, true)
        .unwrap()
        .expect("sends");
    let sent = sent_types(&recorder);
    let stats = sent
        .iter()
        .find(|v| v["@type"] == "getStoryStatistics")
        .expect("statistics request");
    assert_eq!(stats["chat_id"], 7);
    assert_eq!(stats["story_id"], 6);
    assert_eq!(stats["is_dark"], true);
    let forwards = sent
        .iter()
        .find(|v| v["@type"] == "getStoryPublicForwards")
        .expect("forwards request");
    assert_eq!(forwards["story_poster_chat_id"], 7);
    assert_eq!(forwards["offset"], "");
    // A second tap while both are in flight sends nothing new.
    assert!(
        driver
            .fetch_story_insights(ChatId(7), 6, true)
            .unwrap()
            .is_none()
    );
    assert_eq!(sent_types(&recorder).len(), sent.len());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn public_story_search_pages_by_next_offset() {
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
        driver.search_public_stories(StorySearchQuery::Tag("#".into())),
        Err(ConnectSendError::InvalidRequest)
    );
    let extra = driver
        .search_public_stories(StorySearchQuery::Tag("#sunset".into()))
        .unwrap()
        .expect("sends");
    let first = sent_types(&recorder).pop().unwrap();
    assert_eq!(first["@type"], "searchPublicStoriesByTag");
    assert_eq!(first["tag"], "sunset");
    assert_eq!(first["offset"], "");
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"foundStories","@extra":"{}","total_count":2,"next_offset":"p2","stories":[{{"@type":"story","id":1,"poster_chat_id":21,"date":1,"content":{{"@type":"storyContentUnsupported"}}}}]}}"#,
                    extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
        .load_more_found_stories()
        .unwrap()
        .expect("second page");
    let second = sent_types(&recorder).pop().unwrap();
    assert_eq!(second["offset"], "p2");
    // Nothing more to load while the page is in flight.
    assert!(driver.load_more_found_stories().unwrap().is_none());
    let _ = std::fs::remove_dir_all(dir);
}

fn found_stories(extra: crate::ids::RequestId, poster: i64, next: &str) -> String {
    format!(
        r#"{{"@type":"foundStories","@extra":"{}","total_count":1,"next_offset":"{next}","stories":[{{"@type":"story","id":1,"poster_chat_id":{poster},"date":1,"content":{{"@type":"storyContentUnsupported"}}}}]}}"#,
        extra.0
    )
}

/// A new search while the last one's page is in flight goes out at once,
/// and only its own answer fills it: the old answer, its offset and its
/// failure land in nothing.
#[test]
fn a_new_story_search_replaces_one_in_flight() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let cats = driver
        .search_public_stories(StorySearchQuery::Tag("#cats".into()))
        .unwrap()
        .expect("sends");
    let dogs = driver
        .search_public_stories(StorySearchQuery::Tag("#dogs".into()))
        .unwrap()
        .expect("the new query is sent while the old one is in flight");
    assert_eq!(sent_types(&recorder).pop().unwrap()["tag"], "dogs");

    driver
        .ingest(copy_and_parse(&found_stories(cats, 21, "cats-2"), &seq, &dyn_sink).unwrap())
        .unwrap();
    let search = driver.session.stories.search.as_ref().expect("the search");
    assert!(search.stories.is_empty(), "the old answer is not listed");
    assert!(search.next_offset.is_empty());
    assert!(search.loading, "still waiting for its own page");

    driver
        .ingest(copy_and_parse(&found_stories(dogs, 31, "dogs-2"), &seq, &dyn_sink).unwrap())
        .unwrap();
    let search = driver.session.stories.search.as_ref().expect("the search");
    assert_eq!(search.stories, vec![(31, 1)]);
    assert_eq!(search.next_offset, "dogs-2");

    // A failure of a search left behind does not mark the open one.
    let birds = driver
        .search_public_stories(StorySearchQuery::Tag("#birds".into()))
        .unwrap()
        .expect("sends");
    let fish = driver
        .search_public_stories(StorySearchQuery::Tag("#fish".into()))
        .unwrap()
        .expect("sends");
    let error = |id: crate::ids::RequestId| {
        format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"TAG_INVALID"}}"#,
            id.0
        )
    };
    driver
        .ingest(copy_and_parse(&error(birds), &seq, &dyn_sink).unwrap())
        .unwrap();
    let search = driver.session.stories.search.as_ref().expect("the search");
    assert!(search.error.is_none());
    assert!(search.loading);
    driver
        .ingest(copy_and_parse(&error(fish), &seq, &dyn_sink).unwrap())
        .unwrap();
    let search = driver.session.stories.search.as_ref().expect("the search");
    assert!(search.error.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

/// Statistics opened on A, then B, then A again before A's answer: the
/// panel waits for A, and A's answer fills it.
#[test]
fn story_statistics_reopened_while_pending_take_the_late_answer() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    for story in [5, 6] {
        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            story,
            "storyContentPhoto",
            r#""can_get_statistics":true,"#,
        );
    }
    let a = driver
        .fetch_story_insights(ChatId(7), 5, false)
        .unwrap()
        .expect("A sends");
    driver
        .fetch_story_insights(ChatId(7), 6, false)
        .unwrap()
        .expect("B sends");
    assert!(
        driver
            .fetch_story_insights(ChatId(7), 5, false)
            .unwrap()
            .is_none(),
        "A's request is still in flight"
    );
    let insights = driver.session.stories.insights.as_ref().expect("insights");
    assert_eq!((insights.chat_id, insights.story_id), (7, 5));
    assert!(
        insights.forwards_loading(),
        "A's forwards are on the way too"
    );

    let graph = r#"{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""}"#;
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"storyStatistics","@extra":"{}","story_interaction_graph":{graph},"story_reaction_graph":{graph}}}"#,
                    a.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let insights = driver.session.stories.insights.as_ref().expect("insights");
    assert!(
        matches!(
            insights.statistics,
            crate::state::StoryStatsFetch::Loaded(_)
        ),
        "{:?}",
        insights.statistics
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A's forwards answer before A's statistics, B opens, then A again: A's
/// forwards are asked for again instead of showing an empty list.
#[test]
fn story_forwards_answered_before_a_reopen_are_asked_again() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    for story in [5, 6] {
        seed_story(
            &mut driver,
            &seq,
            &dyn_sink,
            7,
            story,
            "storyContentPhoto",
            r#""can_get_statistics":true,"#,
        );
    }
    driver.fetch_story_insights(ChatId(7), 5, false).unwrap();
    let forwards = sent_types(&recorder)
        .into_iter()
        .find(|v| v["@type"] == "getStoryPublicForwards")
        .expect("A's forwards");
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"publicForwards","@extra":"{}","total_count":0,"next_offset":"","forwards":[]}}"#,
                    forwards["@extra"].as_str().unwrap()
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver.fetch_story_insights(ChatId(7), 6, false).unwrap();
    driver.fetch_story_insights(ChatId(7), 5, false).unwrap();
    let asked = sent_types(&recorder)
        .into_iter()
        .filter(|v| v["@type"] == "getStoryPublicForwards" && v["story_id"] == 5)
        .count();
    assert_eq!(asked, 2);
    let insights = driver.session.stories.insights.as_ref().expect("insights");
    assert_eq!((insights.chat_id, insights.story_id), (7, 5));
    assert!(insights.forwards_loading());
    let _ = std::fs::remove_dir_all(dir);
}

/// A's forwards answer first, then A is reopened while its statistics are
/// still out: the first page is not asked for again, so it is not shown
/// twice (sweep 03-007).
#[test]
fn the_first_forwards_page_is_not_asked_twice_on_a_reopen() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        5,
        "storyContentPhoto",
        r#""can_get_statistics":true,"#,
    );
    driver.fetch_story_insights(ChatId(7), 5, false).unwrap();
    let forwards = sent_types(&recorder)
        .into_iter()
        .find(|v| v["@type"] == "getStoryPublicForwards")
        .expect("A's forwards");
    let page = format!(
        r#"{{"@type":"publicForwards","@extra":"{}","total_count":1,"next_offset":"","forwards":[{{"@type":"publicForwardStory","story":{{"@type":"story","id":9,"poster_chat_id":7,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}]}}"#,
        forwards["@extra"].as_str().unwrap()
    );
    driver
        .ingest(copy_and_parse(&page, &seq, &dyn_sink).unwrap())
        .unwrap();
    let rows = |session: &Session| {
        session
            .stories
            .insights
            .as_ref()
            .map_or(0, |state| state.forwards.len())
    };
    assert_eq!(rows(&driver.session), 1);
    driver.fetch_story_insights(ChatId(7), 5, false).unwrap();
    let asked = sent_types(&recorder)
        .into_iter()
        .filter(|v| v["@type"] == "getStoryPublicForwards")
        .count();
    assert_eq!(asked, 1, "the answered first page is not asked for again");
    assert_eq!(rows(&driver.session), 1);
    let _ = std::fs::remove_dir_all(dir);
}
