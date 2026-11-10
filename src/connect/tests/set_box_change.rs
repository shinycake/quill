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
