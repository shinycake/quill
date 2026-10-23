use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{FileId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::StickerSetViewStage;
use crate::sticker_set_box::{ADDED_NOTE, UPLOAD_FAILED_NOTE};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::StickerFormat;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Driver = ConnectDriver<Arc<RecordingSender>>;

fn feed(driver: &mut Driver, seq: &AtomicU64, sink: &Arc<dyn DiagnosticSink>, value: Value) {
    driver
        .ingest(copy_and_parse(&value.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn sticker(id: i32, emoji: &str) -> Value {
    json!({"@type":"sticker","id":id,"set_id":"77","emoji":emoji,"width":512,"height":512,
        "format":{"@type":"stickerFormatWebp"},
        "full_type":{"@type":"stickerFullTypeRegular"},
        "sticker":{"@type":"file","id":id,"local":{"@type":"localFile","can_be_downloaded":true},"remote":{"@type":"remoteFile"}}})
}

fn set_payload(extra: &str, is_owned: bool, ids: &[i32]) -> Value {
    let stickers: Vec<Value> = ids.iter().map(|id| sticker(*id, "😀")).collect();
    json!({"@type":"stickerSet","@extra":extra,"id":"77","title":"Mine","name":"mine_by_me",
        "is_installed":true,"is_owned":is_owned,"sticker_type":{"@type":"stickerTypeRegular"},
        "stickers":stickers,"emojis":[]})
}

fn open_owned_set(
    driver: &mut Driver,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    is_owned: bool,
) {
    let extra = driver.view_sticker_set(77).unwrap();
    feed(
        driver,
        seq,
        sink,
        set_payload(&extra.as_extra(), is_owned, &[1, 2, 3]),
    );
}

fn viewed_ids(driver: &Driver) -> Vec<i32> {
    match &driver
        .session
        .stickers
        .sticker_set_view
        .as_ref()
        .unwrap()
        .stage
    {
        StickerSetViewStage::Ready { stickers, .. } => {
            stickers.iter().map(|s| s.file_id.0).collect()
        }
        other => panic!("not ready: {other:?}"),
    }
}

fn edits(driver: &Driver) -> &crate::state::StickerSetEdits {
    &driver
        .session
        .stickers
        .sticker_set_view
        .as_ref()
        .unwrap()
        .edits
}

fn ok(extra: RequestId) -> Value {
    json!({"@type":"ok","@extra":extra.as_extra()})
}

fn error(extra: RequestId) -> Value {
    json!({"@type":"error","@extra":extra.as_extra(),"code":400,"message":"STICKERSET_INVALID"})
}

#[test]
fn only_an_owned_open_set_can_be_edited() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    assert!(
        driver.rename_sticker_set(77, "Cats").is_err(),
        "no view open"
    );
    open_owned_set(&mut driver, &seq, &sink, false);
    assert!(driver.rename_sticker_set(77, "Cats").is_err(), "not owned");
    assert!(driver.remove_sticker_from_set(77, FileId(1)).is_err());
    assert!(driver.delete_owned_sticker_set(77).is_err());
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("setStickerSetTitle")),
        "nothing was sent for a set we do not own"
    );
    drop(dir);
}

#[test]
fn adding_a_new_sticker_uploads_it_and_refreshes_the_set() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    open_owned_set(&mut driver, &seq, &sink, true);

    assert!(
        driver
            .add_new_sticker_to_set(77, "/tmp/s.png", "", StickerFormat::Webp)
            .is_err(),
        "an emoji is required"
    );
    let extra = driver
        .add_new_sticker_to_set(77, "/tmp/s.png", "😀", StickerFormat::Webp)
        .unwrap();
    let request = sent_request(&recorder, "addStickerToSet");
    assert_eq!(request["name"], "mine_by_me");
    assert_eq!(request["sticker"]["sticker"]["path"], "/tmp/s.png");
    assert_eq!(request["sticker"]["format"]["@type"], "stickerFormatWebp");
    assert!(edits(&driver).busy());
    assert!(
        driver.sync_sticker_set_view().unwrap().is_none(),
        "nothing to refresh yet"
    );

    feed(&mut driver, &seq, &sink, ok(extra));
    assert!(!edits(&driver).busy());
    assert!(edits(&driver).refresh_needed);
    assert_eq!(
        driver.session.take_sticker_set_edit_note().as_deref(),
        Some(ADDED_NOTE)
    );
    assert_eq!(
        driver.session.take_sticker_set_edit_note(),
        None,
        "the toast is one-shot"
    );

    let refresh = driver.sync_sticker_set_view().unwrap().unwrap();
    let request = sent_request(&recorder, "getStickerSet");
    assert_eq!(request["set_id"], "77");
    assert!(!edits(&driver).refresh_needed);
    assert_eq!(
        viewed_ids(&driver),
        vec![1, 2, 3],
        "the box keeps its stickers while refreshing"
    );
    assert!(
        driver.sync_sticker_set_view().unwrap().is_none(),
        "one refresh at a time"
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        set_payload(&refresh.as_extra(), true, &[1, 2, 3, 4]),
    );
    assert_eq!(viewed_ids(&driver), vec![1, 2, 3, 4]);
    drop(dir);
}

#[test]
fn a_failed_upload_shows_tdesktops_wording() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    open_owned_set(&mut driver, &seq, &sink, true);

    let extra = driver
        .add_new_sticker_to_set(77, "/tmp/s.webm", "😀", StickerFormat::Webm)
        .unwrap();
    let request = sent_request(&recorder, "addStickerToSet");
    assert_eq!(request["sticker"]["format"]["@type"], "stickerFormatWebm");
    assert_eq!(request["sticker"]["sticker"]["path"], "/tmp/s.webm");
    feed(&mut driver, &seq, &sink, error(extra));
    assert!(!edits(&driver).busy());
    assert!(!edits(&driver).refresh_needed);
    assert_eq!(
        driver.session.take_sticker_set_edit_note().as_deref(),
        Some(UPLOAD_FAILED_NOTE)
    );
    drop(dir);
}

#[test]
fn an_existing_sticker_is_added_under_its_emoji_or_the_default() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    open_owned_set(&mut driver, &seq, &sink, true);

    driver
        .add_existing_sticker_to_set(77, FileId(900), StickerFormat::Tgs, "🔥")
        .unwrap();
    let request = sent_request(&recorder, "addStickerToSet");
    assert_eq!(request["sticker"]["sticker"]["@type"], "inputFileId");
    assert_eq!(request["sticker"]["sticker"]["id"], 900);
    assert_eq!(request["sticker"]["format"]["@type"], "stickerFormatTgs");
    assert_eq!(request["sticker"]["emojis"], "🔥");

    driver
        .add_existing_sticker_to_set(77, FileId(901), StickerFormat::Webp, "")
        .unwrap();
    let request = sent_request(&recorder, "addStickerToSet");
    assert_eq!(request["sticker"]["emojis"], "🙂");
    assert_eq!(edits(&driver).pending, 2);
    drop(dir);
}

#[test]
fn removing_a_sticker_drops_it_from_the_box_once_confirmed() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    open_owned_set(&mut driver, &seq, &sink, true);
    driver
        .session
        .media
        .media_library
        .set_stickers
        .insert(77, Vec::new());

    let extra = driver.remove_sticker_from_set(77, FileId(2)).unwrap();
    assert_eq!(
        sent_request(&recorder, "removeStickerFromSet")["sticker"]["id"],
        2
    );
    assert_eq!(
        viewed_ids(&driver),
        vec![1, 2, 3],
        "nothing changes until TDLib agrees"
    );
    feed(&mut driver, &seq, &sink, ok(extra));
    assert_eq!(viewed_ids(&driver), vec![1, 3]);
    assert!(edits(&driver).refresh_needed);
    assert!(
        !driver
            .session
            .media
            .media_library
            .set_stickers
            .contains_key(&77),
        "the panel's cached set is forgotten"
    );
    drop(dir);
}

#[test]
fn moving_a_sticker_reorders_the_box_at_once_and_tells_tdlib() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    open_owned_set(&mut driver, &seq, &sink, true);

    assert!(
        driver.move_sticker_in_set(77, 1, 1).unwrap().is_none(),
        "a no-op move sends nothing"
    );
    assert!(
        driver.move_sticker_in_set(77, 5, 0).unwrap().is_none(),
        "out of range is ignored"
    );
    let extra = driver.move_sticker_in_set(77, 0, 2).unwrap().unwrap();
    assert_eq!(viewed_ids(&driver), vec![2, 3, 1]);
    let request = sent_request(&recorder, "setStickerPositionInSet");
    assert_eq!(request["sticker"]["id"], 1);
    assert_eq!(request["position"], 2);
    feed(&mut driver, &seq, &sink, ok(extra));
    assert!(
        !edits(&driver).refresh_needed,
        "a confirmed move does not blink the box"
    );

    let extra = driver.move_sticker_in_set(77, 2, 0).unwrap().unwrap();
    feed(&mut driver, &seq, &sink, error(extra));
    assert!(
        edits(&driver).refresh_needed,
        "a failed move asks for the server's order"
    );
    assert!(driver.session.take_sticker_set_edit_note().is_some());
    drop(dir);
}

#[test]
fn renaming_and_deleting_name_the_set_by_its_short_name() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    open_owned_set(&mut driver, &seq, &sink, true);

    assert!(driver.rename_sticker_set(77, "   ").is_err());
    let extra = driver.rename_sticker_set(77, "  Cats ").unwrap();
    let request = sent_request(&recorder, "setStickerSetTitle");
    assert_eq!(request["name"], "mine_by_me");
    assert_eq!(request["title"], "Cats");
    feed(&mut driver, &seq, &sink, ok(extra));
    assert!(edits(&driver).refresh_needed);
    assert!(
        !driver.session.stickers.stickers.installed_loaded,
        "the installed list is fetched again so the panel shows the new name"
    );

    let extra = driver.delete_owned_sticker_set(77).unwrap();
    assert_eq!(
        sent_request(&recorder, "deleteStickerSet")["name"],
        "mine_by_me"
    );
    assert!(!edits(&driver).deleted);
    feed(&mut driver, &seq, &sink, ok(extra));
    assert!(
        edits(&driver).deleted,
        "the box closes once the set is gone"
    );
    drop(dir);
}

#[test]
fn an_emoji_set_toasts_emoji_added_even_without_a_box() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    // An owned emoji set known only from the installed list ("Add to Emoji
    // Set" in a message's menu).
    driver
        .session
        .stickers
        .emoji
        .installed_sets
        .push(crate::telegram::envelope::StickerSetInfo {
            id: 88,
            title: "My emoji".into(),
            name: "my_emoji_by_me".into(),
            size: 3,
            is_installed: true,
            is_official: false,
            is_owned: true,
            covers: Vec::new(),
            cover_files: Vec::new(),
            is_viewed: true,
        });
    assert_eq!(
        driver.session.owned_sticker_set(88),
        Some(("my_emoji_by_me".to_string(), true))
    );
    let extra = driver
        .add_existing_sticker_to_set(88, FileId(41), StickerFormat::Tgs, "")
        .unwrap();
    let request = sent_request(&recorder, "addStickerToSet");
    assert_eq!(request["name"], "my_emoji_by_me");
    feed(&mut driver, &seq, &sink, ok(extra));
    assert_eq!(
        driver.session.take_sticker_set_edit_note().as_deref(),
        Some(crate::sticker_set_box::EMOJI_ADDED_NOTE)
    );
    // Menus and the panel badge follow the server: both installed lists
    // are fetched again on the next sync, once.
    assert!(driver.sync_sticker_set_view().unwrap().is_none());
    sent_request(&recorder, "getInstalledStickerSets");
    let sent: Vec<Value> = recorder
        .snapshot()
        .iter()
        .filter_map(|json| serde_json::from_str(json).ok())
        .filter(|v: &Value| v["@type"] == "getInstalledStickerSets")
        .collect();
    assert_eq!(sent.len(), 2, "regular and custom emoji sets");
    assert!(
        sent.iter()
            .any(|v| v["sticker_type"]["@type"] == "stickerTypeCustomEmoji")
    );
    assert!(driver.sync_sticker_set_view().unwrap().is_none());
    let again: usize = recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("getInstalledStickerSets"))
        .count();
    assert_eq!(again, 2, "fetched once per edit");
    drop(dir);
}

#[test]
fn a_refresh_answered_during_a_move_asks_again() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    open_owned_set(&mut driver, &seq, &sink, true);
    let rename = driver.rename_sticker_set(77, "Cats").unwrap();
    feed(&mut driver, &seq, &sink, ok(rename));
    let refresh = driver.sync_sticker_set_view().unwrap().unwrap();
    // The user drags a sticker while the refresh is on its way.
    let moved = driver.move_sticker_in_set(77, 0, 2).unwrap().unwrap();
    assert_eq!(viewed_ids(&driver), vec![2, 3, 1]);
    feed(
        &mut driver,
        &seq,
        &sink,
        set_payload(&refresh.as_extra(), true, &[1, 2, 3]),
    );
    assert_eq!(viewed_ids(&driver), vec![1, 2, 3], "the stale answer lands");
    assert!(edits(&driver).refresh_needed, "but a refresh is owed");
    feed(&mut driver, &seq, &sink, ok(moved));
    assert!(driver.sync_sticker_set_view().unwrap().is_some());
    drop(dir);
}
