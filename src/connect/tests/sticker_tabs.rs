use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::FileId;
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, StickerTab};
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn sticker_tabs_requests_failures_and_confirmed_mutations() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.stickers.open = true;
    let recent = driver
        .select_sticker_tab(StickerTab::Recent)
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getRecentStickers")["is_attached"],
        false
    );
    assert_eq!(driver.select_sticker_tab(StickerTab::Recent).unwrap(), None);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"error", "@extra":recent.as_extra(), "code":500,"message":"test"})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.stickers.failed);
    assert!(
        driver
            .select_sticker_tab(StickerTab::Recent)
            .unwrap()
            .is_some()
    );
    assert!(!driver.session.stickers.failed);

    driver.select_sticker_tab(StickerTab::Favorites).unwrap();
    let add = driver
        .set_favorite_sticker(FileId(41), true)
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "addFavoriteSticker")["sticker"]["id"],
        41
    );
    assert_eq!(driver.set_favorite_sticker(FileId(42), true).unwrap(), None);
    // A successful mutation replaces an older pending fetch; its late answer is ignored.
    let fetch = sent_request(&recorder, "getFavoriteStickers");
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":add.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_ne!(
        sent_request(&recorder, "getFavoriteStickers")["@extra"],
        fetch["@extra"]
    );
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"stickers","@extra":fetch["@extra"],"stickers":[]}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetFavoriteStickers)
    );
    assert_eq!(driver.session.stickers.tab, StickerTab::Favorites);
    assert!(driver.set_favorite_sticker(FileId(0), true).is_err());
    let clear = driver.clear_recent_stickers().unwrap().unwrap();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":clear.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetRecentStickers)
    );
    assert!(driver.session.stickers.recent.is_empty());
    assert_eq!(
        sent_request(&recorder, "clearRecentStickers")["is_attached"],
        false
    );

    let trending = driver
        .select_sticker_tab(StickerTab::Trending)
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getTrendingStickerSets")["offset"],
        0
    );
    let sets = json!({"@type":"trendingStickerSets","@extra":trending.as_extra(),"total_count":1,"is_premium":false,"sets":[{"@type":"stickerSetInfo","id":"77","title":"Discover","name":"Discover","size":2}]});
    driver
        .ingest(copy_and_parse(&sets.to_string(), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.trending.len(), 1);
    assert_eq!(
        sent_request(&recorder, "viewTrendingStickerSets")["sticker_set_ids"],
        json!([77])
    );
    let page = driver.fetch_trending_stickers(true).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getTrendingStickerSets")["offset"],
        1
    );
    let sets = json!({"@type":"trendingStickerSets","@extra":page.as_extra(),"total_count":3,"is_premium":false,"sets":[{"@type":"stickerSetInfo","id":"77","title":"Discover","name":"Discover","size":2},{"@type":"stickerSetInfo","id":"88","title":"More","name":"More","size":2}]});
    driver
        .ingest(copy_and_parse(&sets.to_string(), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.trending.len(), 2);
    assert_eq!(driver.session.stickers.trending_next_offset, 3);
    assert_eq!(
        sent_request(&recorder, "viewTrendingStickerSets")["sticker_set_ids"],
        json!([77, 88])
    );
    driver.select_sticker_set(77).unwrap();
    assert_eq!(sent_request(&recorder, "getStickerSet")["set_id"], "77");
    driver.session.stickers.close();
    driver.session.auth = crate::telegram::envelope::AuthorizationState::WaitPhoneNumber;
    assert!(driver.select_sticker_tab(StickerTab::Recent).is_err());
    let _ = std::fs::remove_dir_all(dir);
}
