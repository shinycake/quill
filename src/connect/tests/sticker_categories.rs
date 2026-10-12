use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::EmojiCategorySource;
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn sticker(id: i64) -> serde_json::Value {
    json!({"@type":"sticker","id":id,"set_id":"77","emoji":"😀","format":{"@type":"stickerFormatWebp"},"full_type":{"@type":"stickerFullTypeRegular","premium_animation":{"@type":"file","id":900,"local":{"@type":"localFile"},"remote":{"@type":"remoteFile"}}},"sticker":{"@type":"file","id":id,"local":{"@type":"localFile","can_be_downloaded":false},"remote":{"@type":"remoteFile"}}})
}

fn category_source(
    driver: &ConnectDriver<Arc<RecordingSender>>,
    index: usize,
) -> EmojiCategorySource {
    driver.session.stickers.emoji.sticker_categories[index]
        .source
        .clone()
}

#[test]
fn category_strip_loads_once_and_each_category_searches_its_own_way() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    driver.fetch_sticker_categories().unwrap();
    let request = sent_request(&recorder, "getEmojiCategories");
    assert_eq!(request["type"]["@type"], "emojiCategoryTypeRegularStickers");
    let sent = recorder.snapshot().len();
    driver.fetch_sticker_categories().unwrap();
    assert_eq!(recorder.snapshot().len(), sent, "asked once");

    let answer = json!({"@type":"emojiCategories","@extra":request["@extra"],"categories":[
        {"@type":"emojiCategory","name":"Premium","icon":null,"is_greeting":false,"source":{"@type":"emojiCategorySourcePremium"}},
        {"@type":"emojiCategory","name":"Smile","icon":null,"is_greeting":false,"source":{"@type":"emojiCategorySourceSearch","emojis":["😀","😄"]}},
    ]});
    driver
        .ingest(copy_and_parse(&answer.to_string(), &seq, &sink).unwrap())
        .unwrap();
    let categories = &driver.session.stickers.emoji.sticker_categories;
    assert_eq!(categories.len(), 2);
    assert_eq!(categories[0].source, EmojiCategorySource::Premium);
    assert_eq!(
        categories[1].source,
        EmojiCategorySource::Search(vec!["😀".into(), "😄".into()])
    );
    assert!(
        driver.session.stickers.emoji.categories.is_empty(),
        "the picker's own categories are not replaced"
    );

    // The Premium category asks TDLib for the Premium stickers.
    driver
        .search_sticker_category(&EmojiCategorySource::Premium)
        .unwrap();
    let premium = sent_request(&recorder, "getPremiumStickers");
    assert_eq!(premium["limit"], 100);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"stickers","@extra":premium["@extra"],"stickers":[sticker(1)]})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    let found = &driver.session.stickers.stickers.found_stickers;
    assert_eq!(found.len(), 1);
    assert!(found[0].premium_animation.is_some());

    // A search category sends its emoji, space-separated, and drops the
    // Premium list the moment it is chosen.
    let source = category_source(&driver, 1);
    driver.search_sticker_category(&source).unwrap();
    assert!(driver.session.stickers.stickers.found_stickers.is_empty());
    let search = sent_request(&recorder, "searchStickers");
    assert_eq!(search["emojis"], "😀 😄");
    assert_eq!(search["query"], "");

    // A category without a usable source does nothing.
    assert!(
        driver
            .search_sticker_category(&EmojiCategorySource::Unknown)
            .is_err()
    );
    assert!(
        driver
            .search_sticker_category(&EmojiCategorySource::Search(Vec::new()))
            .is_err()
    );
    let _ = std::fs::remove_dir_all(dir);
}
