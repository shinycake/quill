//! Connect-driver tests: the tab strip of the global search (media tabs on
//! an empty field, paging, recommendations, the Posts quota).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::search_filters::GlobalSearchFilters;
use crate::search_tabs::SearchTab;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    _dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut fx = Self {
            driver,
            recorder,
            sink,
            seq: AtomicU64::new(0),
            _dir: dir,
        };
        fx.feed(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        fx
    }

    fn feed(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn requests(&self, type_name: &str) -> Vec<Value> {
        let needle = format!("\"@type\":\"{type_name}\"");
        self.recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains(&needle))
            .map(|j| serde_json::from_str(j).unwrap())
            .collect()
    }

    fn open_tab(&mut self, tab: SearchTab) {
        let filters = tab.apply(GlobalSearchFilters::default());
        self.driver.set_search_filters(filters).unwrap();
    }

    /// Answer the newest request of `type_name` with `body` (the object's
    /// fields after `@type`/`@extra`).
    fn answer(&mut self, type_name: &str, answer_type: &str, fields: &str) {
        let request = self.requests(type_name).pop().unwrap();
        let extra = request["@extra"].as_str().unwrap().to_string();
        self.feed(&format!(
            r#"{{"@type":"{answer_type}","@extra":"{extra}",{fields}}}"#
        ));
    }
}

fn message(id: i64, chat: i64) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{chat},"date":1791460800,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hit {id}","entities":[]}}}}}}"#
    )
}

#[test]
fn a_media_tab_on_an_empty_field_lists_that_kind_everywhere() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    fx.open_tab(SearchTab::Photos);
    let search = fx.requests("searchMessages");
    assert_eq!(search.len(), 1);
    assert_eq!(search[0]["query"], "");
    assert_eq!(search[0]["filter"]["@type"], "searchMessagesFilterPhoto");
    assert_eq!(search[0]["offset"], "");
    // No chat or public-chat lookups for an empty query.
    assert!(fx.requests("searchPublicChats").is_empty());
    assert!(fx.driver.session.search.search.open);
    assert!(!fx.driver.session.search.search.recents);
}

#[test]
fn clearing_the_field_on_a_media_tab_stays_on_the_tab() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    fx.open_tab(SearchTab::Links);
    commit_typed_search(&mut fx.driver, "rust");
    assert_eq!(
        fx.requests("searchMessages").pop().unwrap()["filter"]["@type"],
        "searchMessagesFilterUrl"
    );
    let outcome = fx.driver.set_search_query("").unwrap();
    assert!(matches!(
        outcome,
        SearchQueryOutcome::Sent(SearchFlight::Media(_))
    ));
    let last = fx.requests("searchMessages").pop().unwrap();
    assert_eq!(last["query"], "");
    assert_eq!(last["filter"]["@type"], "searchMessagesFilterUrl");
    // The list is already on its way: another empty change asks nothing.
    let before = fx.recorder.snapshot().len();
    assert!(matches!(
        fx.driver.set_search_query("").unwrap(),
        SearchQueryOutcome::Unchanged
    ));
    assert_eq!(fx.recorder.snapshot().len(), before);
}

#[test]
fn going_back_to_chats_on_an_empty_field_asks_for_recents() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    let before = fx.requests("searchRecentlyFoundChats").len();
    fx.open_tab(SearchTab::Voice);
    fx.open_tab(SearchTab::Chats);
    assert_eq!(fx.requests("searchRecentlyFoundChats").len(), before + 1);
    assert!(fx.driver.session.search.search.recents);
}

#[test]
fn every_media_tab_sends_its_filter() {
    let expect = [
        (SearchTab::Photos, "searchMessagesFilterPhoto"),
        (SearchTab::Videos, "searchMessagesFilterVideo"),
        (SearchTab::Links, "searchMessagesFilterUrl"),
        (SearchTab::Files, "searchMessagesFilterDocument"),
        (SearchTab::Music, "searchMessagesFilterAudio"),
        (SearchTab::Voice, "searchMessagesFilterVoiceAndVideoNote"),
    ];
    for (tab, constructor) in expect {
        let mut fx = Fixture::new();
        fx.driver.open_search().unwrap();
        commit_typed_search(&mut fx.driver, "x");
        fx.open_tab(tab);
        let sent = fx.requests("searchMessages").pop().unwrap();
        assert_eq!(sent["filter"]["@type"], constructor, "{tab:?}");
        // Photos, Videos and Voice list everything, whatever was typed.
        assert_eq!(sent["query"], if tab.takes_query() { "x" } else { "" });
    }
}

#[test]
fn the_channels_tab_asks_for_recommendations_once() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    fx.open_tab(SearchTab::Channels);
    assert_eq!(fx.requests("getRecommendedChats").len(), 1);
    // A second visit while the answer is pending asks nothing more.
    fx.open_tab(SearchTab::Chats);
    fx.open_tab(SearchTab::Channels);
    assert_eq!(fx.requests("getRecommendedChats").len(), 1);
    fx.answer(
        "getRecommendedChats",
        "chats",
        r#""total_count":2,"chat_ids":[21,22]"#,
    );
    let tabs = &fx.driver.session.search.search.tabs;
    assert_eq!(tabs.recommended, vec![ChatId(21), ChatId(22)]);
    assert!(tabs.recommended_loaded && !tabs.recommended_loading);
    fx.open_tab(SearchTab::Chats);
    fx.open_tab(SearchTab::Channels);
    assert_eq!(fx.requests("getRecommendedChats").len(), 1);
}

#[test]
fn a_channel_query_narrows_the_message_search_to_channels() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    commit_typed_search(&mut fx.driver, "news");
    fx.open_tab(SearchTab::Channels);
    let sent = fx.requests("searchMessages").pop().unwrap();
    assert_eq!(
        sent["chat_type_filter"]["@type"],
        "searchMessagesChatTypeFilterChannel"
    );
    assert!(sent["filter"].is_null());
}

#[test]
fn later_pages_continue_from_next_offset_and_stop_at_the_end() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    commit_typed_search(&mut fx.driver, "dune");
    fx.answer(
        "searchMessages",
        "foundMessages",
        &format!(
            r#""total_count":3,"next_offset":"page2","messages":[{},{}]"#,
            message(30, 7),
            message(29, 7)
        ),
    );
    assert_eq!(fx.driver.session.search.search.messages.len(), 2);
    assert!(fx.driver.session.search.search.tabs.has_more());

    let extra = fx.driver.load_more_search_results().unwrap();
    assert!(extra.is_some());
    // One page at a time.
    assert!(fx.driver.load_more_search_results().unwrap().is_none());
    let asked = fx.requests("searchMessages").pop().unwrap();
    assert_eq!(asked["offset"], "page2");
    assert_eq!(asked["query"], "dune");

    fx.answer(
        "searchMessages",
        "foundMessages",
        &format!(
            r#""total_count":3,"next_offset":"","messages":[{},{}]"#,
            message(29, 7),
            message(28, 8)
        ),
    );
    let ids: Vec<i64> = fx
        .driver
        .session
        .search
        .search
        .messages
        .iter()
        .map(|hit| hit.message_id.0)
        .collect();
    assert_eq!(ids, [30, 29, 28]);
    assert!(fx.driver.load_more_search_results().unwrap().is_none());
}

#[test]
fn a_media_tab_pages_with_its_filter() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    fx.open_tab(SearchTab::Files);
    fx.answer(
        "searchMessages",
        "foundMessages",
        &format!(
            r#""total_count":9,"next_offset":"more","messages":[{}]"#,
            message(5, 3)
        ),
    );
    fx.driver.load_more_search_results().unwrap();
    let asked = fx.requests("searchMessages").pop().unwrap();
    assert_eq!(asked["offset"], "more");
    assert_eq!(asked["filter"]["@type"], "searchMessagesFilterDocument");
}

#[test]
fn a_new_query_throws_the_old_cursor_away() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    commit_typed_search(&mut fx.driver, "dune");
    fx.answer(
        "searchMessages",
        "foundMessages",
        &format!(
            r#""total_count":3,"next_offset":"page2","messages":[{}]"#,
            message(30, 7)
        ),
    );
    commit_typed_search(&mut fx.driver, "arrakis");
    assert!(fx.driver.load_more_search_results().unwrap().is_none());
    // The old query's late page is dropped by the generation check.
    assert!(fx.driver.session.search.search.messages.is_empty());
}

#[test]
fn the_posts_tab_pages_and_reads_the_quota_without_a_paid_search() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    commit_typed_search(&mut fx.driver, "dune");
    fx.open_tab(SearchTab::Posts);
    let limits = fx.requests("getPublicPostSearchLimits");
    assert_eq!(limits.len(), 1);
    assert_eq!(limits[0]["query"], "dune");
    fx.answer(
        "getPublicPostSearchLimits",
        "publicPostSearchLimits",
        r#""daily_free_query_count":5,"remaining_free_query_count":3,"next_free_query_in":0,"star_count":25,"is_current_query_free":true"#,
    );
    let quota = fx.driver.session.search.search.tabs.limits.unwrap();
    assert_eq!(quota.remaining_free, 3);
    assert_eq!(quota.quota_line(), "3 free searches left today");

    let posts = fx.requests("searchPublicPosts").pop().unwrap();
    assert_eq!(posts["star_count"], 0);
    fx.answer(
        "searchPublicPosts",
        "foundPublicPosts",
        &format!(
            r#""next_offset":"p2","are_limits_exceeded":false,"search_limits":{{"@type":"publicPostSearchLimits"}},"messages":[{}]"#,
            message(90, 400)
        ),
    );
    fx.driver.load_more_search_results().unwrap();
    let more = fx.requests("searchPublicPosts").pop().unwrap();
    assert_eq!(more["offset"], "p2");
    assert_eq!(more["star_count"], 0, "later pages never spend stars");
    // Every searchPublicPosts request of the run was free.
    assert!(
        fx.requests("searchPublicPosts")
            .iter()
            .all(|request| request["star_count"] == 0)
    );
}

#[test]
fn a_failed_page_keeps_the_cursor_for_a_retry() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    commit_typed_search(&mut fx.driver, "dune");
    fx.answer(
        "searchMessages",
        "foundMessages",
        &format!(
            r#""total_count":3,"next_offset":"page2","messages":[{}]"#,
            message(30, 7)
        ),
    );
    fx.driver.load_more_search_results().unwrap();
    fx.answer("searchMessages", "error", r#""code":500,"message":"boom""#);
    let tabs = &fx.driver.session.search.search.tabs;
    assert!(tabs.more_failed && !tabs.loading_more);
    assert!(fx.driver.load_more_search_results().unwrap().is_some());
}

#[test]
fn photos_videos_and_voice_ignore_what_is_typed() {
    for tab in [SearchTab::Photos, SearchTab::Videos, SearchTab::Voice] {
        let mut fx = Fixture::new();
        fx.driver.open_search().unwrap();
        fx.open_tab(tab);
        let before = fx.recorder.snapshot().len();
        let outcome = fx.driver.set_search_query("sunset").unwrap();
        assert!(matches!(outcome, SearchQueryOutcome::Unchanged), "{tab:?}");
        assert_eq!(fx.recorder.snapshot().len(), before, "nothing is sent");
        // The text is remembered for the tabs that use it.
        assert_eq!(fx.driver.session.search.search.query, "sunset");
        fx.open_tab(SearchTab::Links);
        let sent = fx.requests("searchMessages").pop().unwrap();
        assert_eq!(sent["query"], "sunset");
        assert_eq!(sent["filter"]["@type"], "searchMessagesFilterUrl");
    }
}

#[test]
fn links_files_and_music_take_the_typed_query() {
    for tab in [SearchTab::Links, SearchTab::Files, SearchTab::Music] {
        let mut fx = Fixture::new();
        fx.driver.open_search().unwrap();
        fx.open_tab(tab);
        commit_typed_search(&mut fx.driver, "report");
        let sent = fx.requests("searchMessages").pop().unwrap();
        assert_eq!(sent["query"], "report", "{tab:?}");
    }
}

#[test]
fn the_downloads_tab_sends_nothing() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    let before = fx.recorder.snapshot().len();
    fx.open_tab(SearchTab::Downloads);
    let SearchQueryOutcome::Debounced { token } = fx.driver.set_search_query("invoice").unwrap()
    else {
        panic!("expected a debounced query");
    };
    assert!(fx.driver.commit_debounced_search(token).unwrap().is_none());
    let sent = fx.recorder.snapshot();
    assert!(
        sent.iter().skip(before).all(|json| {
            !json.contains("searchMessages")
                && !json.contains("searchChats")
                && !json.contains("searchPublicPosts")
        }),
        "{:?}",
        &sent[before..]
    );
}

fn exceeded_posts_search(fx: &mut Fixture) {
    fx.driver.open_search().unwrap();
    commit_typed_search(&mut fx.driver, "dune");
    fx.open_tab(SearchTab::Posts);
    fx.answer(
        "getPublicPostSearchLimits",
        "publicPostSearchLimits",
        r#""daily_free_query_count":5,"remaining_free_query_count":0,"next_free_query_in":3600,"star_count":25,"is_current_query_free":false"#,
    );
}

#[test]
fn a_paid_search_is_refused_while_free_ones_remain() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    commit_typed_search(&mut fx.driver, "dune");
    fx.open_tab(SearchTab::Posts);
    fx.answer(
        "getPublicPostSearchLimits",
        "publicPostSearchLimits",
        r#""daily_free_query_count":5,"remaining_free_query_count":3,"next_free_query_in":0,"star_count":25,"is_current_query_free":true"#,
    );
    assert!(fx.driver.search_public_posts_paid().unwrap().is_none());
    assert!(
        fx.requests("searchPublicPosts")
            .iter()
            .all(|request| request["star_count"] == 0)
    );
}

#[test]
fn the_paid_search_sends_the_reported_price_only_when_called() {
    let mut fx = Fixture::new();
    exceeded_posts_search(&mut fx);
    // The free attempt was refused: the quota is flagged exceeded.
    fx.answer(
        "searchPublicPosts",
        "foundPublicPosts",
        r#""next_offset":"","are_limits_exceeded":true,"messages":[]"#,
    );
    assert!(fx.driver.session.search.search.public_limits_exceeded);
    // Nothing paid went out on its own.
    assert!(
        fx.requests("searchPublicPosts")
            .iter()
            .all(|request| request["star_count"] == 0)
    );
    // The explicit paid search carries the price and the same query.
    let flight = fx.driver.search_public_posts_paid().unwrap();
    assert!(matches!(flight, Some(SearchFlight::PublicPosts(_))));
    let paid = fx.requests("searchPublicPosts").pop().unwrap();
    assert_eq!(paid["star_count"], 25);
    assert_eq!(paid["query"], "dune");
    assert_eq!(paid["offset"], "");
}

#[test]
fn a_paid_search_needs_a_plain_query_and_a_price() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    fx.driver
        .search_hashtag("#dune", crate::search_filters::SearchScope::PublicPosts)
        .unwrap();
    fx.driver.session.search.search.public_limits_exceeded = true;
    fx.driver.session.search.search.tabs.limits = Some(crate::state::PublicPostLimits {
        daily_free: 5,
        remaining_free: 0,
        next_free_in: 60,
        star_count: 25,
        current_query_free: false,
    });
    // A tag search is never paid.
    assert!(fx.driver.search_public_posts_paid().unwrap().is_none());
    assert!(
        fx.requests("searchPublicPosts")
            .iter()
            .all(|request| request["star_count"] == 0)
    );
}
