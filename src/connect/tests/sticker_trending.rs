use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::TrendingPage;
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn page(extra: crate::ids::RequestId, total: u32, ids: &[&str]) -> String {
    let sets: Vec<_> = ids
        .iter()
        .map(|id| json!({"@type":"stickerSetInfo","id":id,"title":id,"name":id,"size":2}))
        .collect();
    json!({"@type":"trendingStickerSets","@extra":extra.as_extra(),"total_count":total,"is_premium":false,"sets":sets}).to_string()
}

#[test]
fn a_first_page_asked_while_a_later_one_is_out_replaces_it() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let mut ingest = |driver: &mut ConnectDriver<_>, json: String| {
        driver
            .ingest(copy_and_parse(&json, &seq, &sink).unwrap())
            .unwrap();
    };
    // The first page lands, and says more follow.
    let first = driver.fetch_trending_stickers(false).unwrap().unwrap();
    assert_eq!(driver.fetch_trending_stickers(false).unwrap(), None);
    ingest(&mut driver, page(first, 4, &["1", "2"]));
    // Page two goes out; a next page waits for it.
    let second = driver.fetch_trending_stickers(true).unwrap().unwrap();
    assert_eq!(driver.fetch_trending_stickers(true).unwrap(), None);
    // A first page asked meanwhile goes out and replaces it.
    let restart = driver.fetch_trending_stickers(false).unwrap().unwrap();
    assert_ne!(restart, second);
    assert_eq!(driver.fetch_trending_stickers(false).unwrap(), None);
    // The answer of page two lands nowhere.
    ingest(&mut driver, page(second, 4, &["3", "4"]));
    let trending = &driver.session.stickers.stickers.trending;
    assert_eq!(trending.sets.len(), 2, "page two did not land");
    assert!(matches!(trending.page, TrendingPage::Asking { from: 0, .. }));
    // The first page lands and replaces the list.
    ingest(&mut driver, page(restart, 3, &["7"]));
    let trending = &driver.session.stickers.stickers.trending;
    assert_eq!(trending.sets.len(), 1);
    assert_eq!(trending.sets[0].id, 7);
    assert_eq!(trending.page, TrendingPage::More { next: 1 });
    let _ = std::fs::remove_dir_all(dir);
}
