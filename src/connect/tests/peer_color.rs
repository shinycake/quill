use super::super::{ConnectDriver, RecordingSender};
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn sticker(id: i64, file: i32) -> serde_json::Value {
    json!({"@type":"sticker","full_type":{"@type":"stickerFullTypeCustomEmoji","custom_emoji_id":id.to_string()},"set_id":"1","width":100,"height":100,"emoji":"x","format":{"@type":"stickerFormatWebp"},"sticker":{"@type":"file","id":file,"size":5,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false}}})
}

#[test]
fn colors_need_premium_and_icons_load_once() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: serde_json::Value| {
        driver
            .ingest(copy_and_parse(&payload.to_string(), &seq, &sink).unwrap())
            .unwrap();
    };

    // Without Premium nothing is sent.
    assert!(driver.set_accent_color(2, 0).is_err());
    assert!(driver.set_profile_colors(1, 0).is_err());

    driver.session.my_user_id = Some(7);
    ingest(
        &mut driver,
        json!({"@type":"updateUser","user":{"@type":"user","id":7,"first_name":"Test","last_name":"","phone_number":"","is_premium":true,"type":{"@type":"userTypeRegular"}}}),
    );
    assert!(driver.set_accent_color(-1, 0).is_err());
    driver.set_accent_color(9, 55).unwrap();
    let sent = sent_request(&recorder, "setAccentColor");
    assert_eq!(sent["accent_color_id"], 9);
    assert_eq!(sent["background_custom_emoji_id"], 55);
    driver.set_profile_colors(-1, 0).unwrap();
    let sent = sent_request(&recorder, "setProfileAccentColor");
    assert_eq!(sent["profile_accent_color_id"], -1);

    // Icons: one request, then the answer fills the list and keeps only
    // custom emoji.
    let request = driver.load_background_emoji_icons().unwrap().unwrap();
    assert!(driver.load_background_emoji_icons().unwrap().is_none());
    ingest(
        &mut driver,
        json!({"@type":"stickers","@extra":request.as_extra(),"stickers":[sticker(41, 301), sticker(42, 302)]}),
    );
    let icons = &driver.session.users_state.background_emoji_icons;
    assert_eq!(icons.len(), 2);
    assert_eq!(icons[0].custom_emoji_id, Some(41));
    assert!(driver.load_background_emoji_icons().unwrap().is_none());
    driver.download_background_emoji_icons();
    assert_eq!(sent_request(&recorder, "downloadFile")["file_id"], 302);
}
