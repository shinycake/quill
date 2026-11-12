use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| {
            serde_json::from_str::<Value>(json).is_ok_and(|value| value["@type"] == type_name)
        })
        .count()
}

fn answer(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    body: Value,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    driver
        .ingest(copy_and_parse(&body.to_string(), seq, sink).unwrap())
        .unwrap();
}

#[test]
fn closing_removes_sets_one_by_one_then_sends_the_order() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .commit_sticker_manager(false, vec![22, 44], Some(vec![33, 22, 11, 44]))
        .unwrap();
    let first = sent_request(&recorder, "changeStickerSet");
    assert_eq!(first["set_id"], "22");
    assert_eq!(first["is_installed"], false);
    assert_eq!(first["is_archived"], false);
    assert_eq!(count(&recorder, "changeStickerSet"), 1, "one at a time");
    assert_eq!(count(&recorder, "reorderInstalledStickerSets"), 0);
    answer(
        &mut driver,
        json!({"@type":"ok","@extra":first["@extra"]}),
        &seq,
        &sink,
    );
    let second = sent_request(&recorder, "changeStickerSet");
    assert_eq!(second["set_id"], "44");
    assert_eq!(count(&recorder, "reorderInstalledStickerSets"), 0);
    answer(
        &mut driver,
        json!({"@type":"ok","@extra":second["@extra"]}),
        &seq,
        &sink,
    );
    assert_eq!(
        sent_request(&recorder, "reorderInstalledStickerSets")["sticker_set_ids"],
        json!([33, 11])
    );
}

#[test]
fn a_failed_removal_does_not_stop_the_rest() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .commit_sticker_manager(false, vec![22], Some(vec![33, 22, 11]))
        .unwrap();
    let removal = sent_request(&recorder, "changeStickerSet");
    answer(
        &mut driver,
        json!({"@type":"error","@extra":removal["@extra"],"code":500,"message":"test"}),
        &seq,
        &sink,
    );
    assert_eq!(
        sent_request(&recorder, "reorderInstalledStickerSets")["sticker_set_ids"],
        json!([33, 11])
    );
}

#[test]
fn an_untouched_box_sends_nothing() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let before = recorder.snapshot().len();
    driver.commit_sticker_manager(false, vec![], None).unwrap();
    assert_eq!(recorder.snapshot().len(), before);
}

#[test]
fn a_reorder_alone_goes_out_at_once() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .commit_sticker_manager(false, vec![], Some(vec![3, 1, 2]))
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "reorderInstalledStickerSets")["sticker_set_ids"],
        json!([3, 1, 2])
    );
}

#[test]
fn mask_changes_use_the_mask_requests() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .commit_sticker_manager(true, vec![5], Some(vec![7, 5, 6]))
        .unwrap();
    let removal = sent_request(&recorder, "changeStickerSet");
    assert_eq!(removal["set_id"], "5");
    answer(
        &mut driver,
        json!({"@type":"ok","@extra":removal["@extra"]}),
        &seq,
        &sink,
    );
    let reorder = sent_request(&recorder, "reorderInstalledStickerSets");
    assert_eq!(reorder["sticker_set_ids"], json!([7, 6]));
    assert_eq!(reorder["sticker_type"]["@type"], "stickerTypeMask");
}

#[test]
fn viewing_trending_sends_only_the_unread_sets_and_clears_their_dots() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let set = |id: i64, viewed: bool| crate::telegram::envelope::StickerSetInfo {
        id,
        title: String::new(),
        name: String::new(),
        size: 1,
        is_installed: false,
        is_official: false,
        is_owned: false,
        covers: Vec::new(),
        cover_files: Vec::new(),
        is_viewed: viewed,
    };
    driver.session.stickers.stickers.trending.sets =
        vec![set(1, true), set(2, false), set(3, false)];
    assert!(driver.view_shown_trending_stickers().unwrap().is_some());
    assert_eq!(
        sent_request(&recorder, "viewTrendingStickerSets")["sticker_set_ids"],
        json!([2, 3])
    );
    assert!(
        driver
            .session
            .stickers
            .stickers
            .trending
            .sets
            .iter()
            .all(|set| set.is_viewed)
    );
    assert_eq!(driver.view_shown_trending_stickers().unwrap(), None);
}

/// A commit that cannot even be queued (no chats path yet) fails and
/// leaves nothing behind, so the box can keep its edits for another try;
/// one that is queued never fails the call, its lost step is reported once
/// through the manager's failure instead.
#[test]
fn a_commit_fails_the_call_only_when_nothing_was_queued() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    assert!(
        driver
            .commit_sticker_manager(false, vec![22], Some(vec![3, 1, 2]))
            .is_err()
    );
    assert!(driver.session.stickers.manager.commits.is_empty());
    assert_eq!(driver.session.stickers.manager.failure, None);
}

/// A commit whose first send is refused is taken back and the call fails,
/// so the box keeps its edits (sweep 22-005); the same commit goes out and
/// is accepted once the link is back.
#[test]
fn a_commit_whose_first_send_is_refused_fails_the_call_and_is_taken_back() {
    use super::flood_retry_lost::SwitchSender;
    use std::sync::atomic::{AtomicBool, Ordering};
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let down = Arc::new(AtomicBool::new(false));
    let inner = Arc::new(RecordingSender::new());
    let sender = SwitchSender {
        inner: inner.clone(),
        down: down.clone(),
    };
    let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, sender, test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    down.store(true, Ordering::SeqCst);
    assert!(
        driver
            .commit_sticker_manager(false, vec![22], None)
            .is_err()
    );
    assert!(driver.session.stickers.manager.commits.is_empty());
    assert_eq!(driver.session.stickers.manager.failure, None);
    down.store(false, Ordering::SeqCst);
    driver
        .commit_sticker_manager(false, vec![22], None)
        .unwrap();
    assert_eq!(count(&inner, "changeStickerSet"), 1);
}

/// Taking back a refused first commit leaves a failure from the commit
/// before it, which the UI has not shown yet, in place (sweep r3-12).
#[test]
fn taking_back_a_refused_commit_keeps_an_earlier_unshown_failure() {
    use super::flood_retry_lost::SwitchSender;
    use std::sync::atomic::{AtomicBool, Ordering};
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let down = Arc::new(AtomicBool::new(false));
    let inner = Arc::new(RecordingSender::new());
    let sender = SwitchSender {
        inner: inner.clone(),
        down: down.clone(),
    };
    let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, sender, test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver.session.stickers.manager.failure = Some(crate::state::CommitFailure { masks: true });
    down.store(true, Ordering::SeqCst);
    assert!(
        driver
            .commit_sticker_manager(false, vec![22], None)
            .is_err()
    );
    assert!(driver.session.stickers.manager.commits.is_empty());
    assert_eq!(
        driver.session.stickers.manager.failure,
        Some(crate::state::CommitFailure { masks: true })
    );
}
