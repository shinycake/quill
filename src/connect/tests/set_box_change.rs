//! The sticker set box's Add / Remove / Archive waits for TDLib's answer:
//! a request skipped as busy is not a success, and a refusal leaves the
//! box in place with the failure to report.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::{SetChange, SetChangeStage, StickerSetView};
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn answer(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    request: crate::ids::RequestId,
    body: serde_json::Value,
) {
    let mut body = body;
    body["@extra"] = json!(request.as_extra());
    driver
        .ingest(copy_and_parse(&body.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn change(driver: &ConnectDriver<Arc<RecordingSender>>) -> Option<SetChangeStage> {
    driver
        .session
        .stickers
        .sticker_set_view
        .as_ref()
        .map(|view| view.change)
}

#[test]
fn the_box_closes_on_the_answer_and_not_on_a_request_skipped_as_busy() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.stickers.sticker_set_view = Some(StickerSetView::loading(77));

    let install = driver.manage_sticker_set(77, true, false).unwrap().unwrap();
    driver.session.begin_set_change(77, SetChange::Install);
    assert_eq!(
        change(&driver),
        Some(SetChangeStage::Working(SetChange::Install))
    );
    // A second press while it is on its way sends nothing: the box has
    // nothing to report as done.
    assert_eq!(driver.manage_sticker_set(77, false, true).unwrap(), None);
    assert_eq!(
        change(&driver),
        Some(SetChangeStage::Working(SetChange::Install))
    );

    answer(&mut driver, &seq, &sink, install, json!({"@type":"ok"}));
    assert_eq!(
        change(&driver),
        Some(SetChangeStage::Done(SetChange::Install))
    );
    driver.session.settle_set_change();
    assert_eq!(change(&driver), Some(SetChangeStage::Idle));

    // A refusal is reported as failed, never as done.
    let remove = driver
        .manage_sticker_set(77, false, false)
        .unwrap()
        .unwrap();
    driver.session.begin_set_change(77, SetChange::Remove);
    answer(
        &mut driver,
        &seq,
        &sink,
        remove,
        json!({"@type":"error","code":500,"message":"test"}),
    );
    assert_eq!(
        change(&driver),
        Some(SetChangeStage::Failed(SetChange::Remove))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_answer_for_another_change_does_not_finish_the_one_waited_on() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.stickers.sticker_set_view = Some(StickerSetView::loading(77));
    driver.session.begin_set_change(77, SetChange::Archive);
    driver
        .session
        .finish_set_change(77, SetChange::Install, true);
    assert_eq!(
        change(&driver),
        Some(SetChangeStage::Working(SetChange::Archive))
    );
    driver
        .session
        .finish_set_change(78, SetChange::Archive, true);
    assert_eq!(
        change(&driver),
        Some(SetChangeStage::Working(SetChange::Archive))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refused_step_of_the_managers_saved_change_is_reported_once() {
    use crate::state::CommitFailure;
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    // The box closed with a removal and a new order.
    driver
        .commit_sticker_manager(false, vec![5], Some(vec![1, 2, 3]))
        .unwrap();
    let removal = sent_request(&recorder, "changeStickerSet");
    let extra: crate::ids::RequestId = removal["@extra"].as_str().unwrap().parse().unwrap();
    assert_eq!(driver.session.stickers.manager.failure, None);
    // The removal is refused: the change is not lost without a word.
    answer(
        &mut driver,
        &seq,
        &sink,
        extra,
        json!({"@type":"error","code":500,"message":"test"}),
    );
    assert_eq!(
        driver.session.stickers.manager.failure,
        Some(CommitFailure { masks: false })
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A set box's own refused change, answered while the manager's saved
/// change waits behind it, is not a failure of the manager's change.
#[test]
fn a_set_boxs_refusal_is_not_reported_as_the_managers() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let box_change = driver.manage_sticker_set(77, true, false).unwrap().unwrap();
    // The manager closed meanwhile: its removal waits for the box's.
    driver.commit_sticker_manager(false, vec![5], None).unwrap();
    answer(
        &mut driver,
        &seq,
        &sink,
        box_change,
        json!({"@type":"error","code":500,"message":"test"}),
    );
    assert_eq!(driver.session.stickers.manager.failure, None);
    // Its own step then goes out, and its refusal is reported.
    let removal = sent_request(&recorder, "changeStickerSet");
    let extra: crate::ids::RequestId = removal["@extra"].as_str().unwrap().parse().unwrap();
    assert_ne!(extra, box_change, "the manager's step is its own request");
    answer(
        &mut driver,
        &seq,
        &sink,
        extra,
        json!({"@type":"error","code":500,"message":"test"}),
    );
    assert_eq!(
        driver.session.stickers.manager.failure,
        Some(crate::state::CommitFailure { masks: false })
    );
    let _ = std::fs::remove_dir_all(dir);
}
