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

#[test]
fn wearable_gifts_load_and_wearing_one_sends_its_colors() {
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
    driver.session.my_user_id = Some(7);
    ingest(
        &mut driver,
        json!({"@type":"updateUser","user":{"@type":"user","id":7,"first_name":"Test","last_name":"","phone_number":"","is_premium":true,"upgraded_gift_colors":{"@type":"upgradedGiftColors","id":"900"},"type":{"@type":"userTypeRegular"}}}),
    );
    assert_eq!(
        driver
            .session
            .user(7)
            .map(|user| user.upgraded_gift_colors_id),
        Some(900)
    );

    let request = driver.load_wearable_gifts().unwrap().unwrap();
    assert!(driver.load_wearable_gifts().unwrap().is_none());
    let sent = sent_request(&recorder, "getReceivedGifts");
    assert_eq!(sent["exclude_without_colors"], true);
    assert_eq!(sent["owner_id"]["user_id"], 7);
    let gift = |id: &str, colors: serde_json::Value| json!({"@type":"receivedGift","received_gift_id":id,"date":1,"gift":{"@type":"sentGiftUpgraded","gift":{"@type":"upgradedGift","title":"Plush","number":5,"colors":colors}}});
    ingest(
        &mut driver,
        json!({"@type":"receivedGifts","@extra":request.as_extra(),"total_count":2,"next_offset":"","gifts":[
            gift("a", json!({"@type":"upgradedGiftColors","id":"901","model_custom_emoji_id":"5","symbol_custom_emoji_id":"6","light_theme_accent_color":255,"light_theme_colors":[255],"dark_theme_accent_color":65280,"dark_theme_colors":[65280]})),
            gift("b", json!(null)),
        ]}),
    );
    let gifts = &driver.session.users_state.wearable_gifts;
    assert_eq!(gifts.len(), 1);
    assert_eq!(gifts[0].gift.colors.as_ref().map(|c| c.id), Some(901));
    assert!(driver.session.users_state.wearable_gifts_loaded);

    assert!(driver.set_upgraded_gift_colors(0).is_err());
    driver.set_upgraded_gift_colors(901).unwrap();
    let sent = sent_request(&recorder, "setUpgradedGiftColors");
    assert_eq!(sent["upgraded_gift_colors_id"], 901);
}
