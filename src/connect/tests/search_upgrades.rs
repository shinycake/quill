//! Connect-driver tests: search upgrades (server chats, public posts,
//! hashtag scopes, frequent contacts, removing recents).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::search_filters::{GlobalSearchFilters, SearchDateRange, SearchScope};
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
}

#[test]
fn a_typed_search_also_asks_the_server_for_chats() {
    let mut fx = Fixture::new();
    commit_typed_search(&mut fx.driver, "alice");
    let server = fx.requests("searchChatsOnServer");
    assert_eq!(server.len(), 1);
    assert_eq!(server[0]["query"], "alice");
    // It rides along: the three gating requests are still all sent.
    assert_eq!(fx.requests("searchChats").len(), 1);
    assert_eq!(fx.requests("searchPublicChats").len(), 1);
    assert_eq!(fx.requests("searchMessages").len(), 1);
}

#[test]
fn archived_and_older_filters_reach_search_messages() {
    let mut fx = Fixture::new();
    commit_typed_search(&mut fx.driver, "dune");
    fx.driver
        .set_search_filters(GlobalSearchFilters {
            archived: true,
            date: SearchDateRange::Older,
            ..GlobalSearchFilters::default()
        })
        .unwrap();
    let search = fx.requests("searchMessages").pop().unwrap();
    assert_eq!(search["chat_list"]["@type"], "chatListArchive");
    assert_eq!(search["min_date"], 0);
    assert!(search["max_date"].as_i64().unwrap() > 0);
}

#[test]
fn public_posts_scope_sends_only_the_posts_request() {
    let mut fx = Fixture::new();
    fx.driver
        .set_search_query("dune")
        .map(|outcome| match outcome {
            SearchQueryOutcome::Debounced { token } => {
                fx.driver.commit_debounced_search(token).unwrap();
            }
            other => panic!("{other:?}"),
        })
        .unwrap();
    let before = fx.recorder.snapshot().len();
    fx.driver
        .set_search_filters(GlobalSearchFilters {
            scope: SearchScope::PublicPosts,
            ..GlobalSearchFilters::default()
        })
        .unwrap();
    let sent = fx.recorder.snapshot();
    let new: Vec<&String> = sent.iter().skip(before).collect();
    assert_eq!(new.len(), 1, "{new:?}");
    let posts: Value = serde_json::from_str(new[0]).unwrap();
    assert_eq!(posts["@type"], "searchPublicPosts");
    assert_eq!(posts["query"], "dune");
    assert_eq!(posts["star_count"], 0);
}

#[test]
fn a_hashtag_click_searches_the_tag_in_the_chosen_scope() {
    let mut fx = Fixture::new();
    let flight = fx
        .driver
        .search_hashtag("#dune", SearchScope::PublicPosts)
        .unwrap();
    assert!(matches!(flight, Some(SearchFlight::PublicPosts(_))));
    let tag = fx.requests("searchPublicMessagesByTag");
    assert_eq!(tag.len(), 1);
    assert_eq!(tag[0]["tag"], "#dune");
    assert!(fx.requests("searchPublicPosts").is_empty());
    assert!(fx.driver.session.search.open);

    let flight = fx
        .driver
        .search_hashtag("#dune", SearchScope::MyMessages)
        .unwrap();
    assert!(matches!(flight, Some(SearchFlight::Query(..))));
    let mine = fx.requests("searchMessages").pop().unwrap();
    assert_eq!(mine["query"], "#dune");
}

#[test]
fn a_plain_query_in_the_public_scope_is_not_a_tag_search() {
    let mut fx = Fixture::new();
    fx.driver
        .search_hashtag("two words", SearchScope::PublicPosts)
        .unwrap();
    assert!(fx.requests("searchPublicMessagesByTag").is_empty());
    assert_eq!(fx.requests("searchPublicPosts").len(), 1);
}

#[test]
fn opening_search_loads_frequent_contacts_unless_disabled() {
    let mut fx = Fixture::new();
    fx.driver.open_search().unwrap();
    let top = fx.requests("getTopChats");
    assert_eq!(top.len(), 1);
    assert_eq!(top[0]["category"]["@type"], "topChatCategoryUsers");
    // Answer it, so a later open is free to ask again.
    let extra = top[0]["@extra"].as_str().unwrap().to_string();
    fx.feed(&format!(
        r#"{{"@type":"chats","@extra":"{extra}","total_count":0,"chat_ids":[]}}"#
    ));

    fx.driver.set_top_chats_disabled(true).unwrap();
    let option = fx.requests("setOption").pop().unwrap();
    assert_eq!(option["name"], "disable_top_chats");
    assert_eq!(option["value"]["value"], true);
    fx.driver.close_search();
    fx.driver.open_search().unwrap();
    assert_eq!(fx.requests("getTopChats").len(), 1);

    fx.driver.set_top_chats_disabled(false).unwrap();
    assert_eq!(fx.requests("setOption").len(), 2);
    fx.driver.close_search();
    fx.driver.open_search().unwrap();
    assert_eq!(fx.requests("getTopChats").len(), 2);
}

#[test]
fn removing_a_frequent_contact_is_optimistic() {
    let mut fx = Fixture::new();
    fx.driver.session.search.top_chats = vec![ChatId(11), ChatId(12)];
    fx.driver.remove_top_chat(ChatId(11)).unwrap();
    assert_eq!(fx.driver.session.search.top_chats, vec![ChatId(12)]);
    let sent = fx.requests("removeTopChat");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_id"], 11);
    // Already gone: nothing more is sent.
    assert_eq!(fx.driver.remove_top_chat(ChatId(11)).unwrap(), None);
    assert_eq!(fx.requests("removeTopChat").len(), 1);
}

#[test]
fn removing_one_recent_search_sends_remove_recently_found_chat() {
    let mut fx = Fixture::new();
    fx.driver.session.search.open = true;
    fx.driver.session.search.recents = true;
    fx.driver.session.search.chat_ids = vec![ChatId(7), ChatId(8)];
    fx.driver.remove_recent_search(ChatId(7)).unwrap();
    assert_eq!(fx.driver.session.search.chat_ids, vec![ChatId(8)]);
    let sent = fx.requests("removeRecentlyFoundChat");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_id"], 7);
    assert_eq!(fx.driver.remove_recent_search(ChatId(7)).unwrap(), None);
}
