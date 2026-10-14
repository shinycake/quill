//! Connect-driver tests: the inline results panel (`@gif cats`) downloads
//! its images as soon as the results arrive, ahead of the chat's own
//! thumbnails.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn file(id: i64, size: i64) -> Value {
    json!({"@type": "file", "id": id, "size": size, "expected_size": size,
        "local": {"@type": "localFile", "path": "", "can_be_downloaded": true,
            "is_downloading_active": false, "is_downloading_completed": false,
            "downloaded_size": 0},
        "remote": {"@type": "remoteFile", "id": format!("r{id}"), "unique_id": format!("u{id}")}})
}

fn animation(id: &str, clip: i64, thumb: i64, format: &str) -> Value {
    json!({"@type": "inlineQueryResultAnimation", "id": id, "title": "",
        "animation": {"@type": "animation", "duration": 3, "width": 320, "height": 240,
            "file_name": "", "mime_type": "video/mp4",
            "thumbnail": {"@type": "thumbnail", "format": {"@type": format},
                "width": 150, "height": 112, "file": file(thumb, 20_000)},
            "animation": file(clip, 300_000)}})
}

/// Regression: the results arrived in a fraction of a second but no
/// `downloadFile` followed until some unrelated update ran the download
/// pass, so the grid stayed grey (or never filled).
#[test]
fn inline_results_download_their_images_right_away() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    let extra = driver
        .inline_query(77, ChatId(1), "cats", "")
        .expect("query")
        .expect("request id");
    let results = json!({
        "@type": "inlineQueryResults", "@extra": extra.0.to_string(),
        "inline_query_id": "5", "button": null, "next_offset": "",
        "results": [
            animation("a", 10, 11, "thumbnailFormatJpeg"),
            animation("b", 20, 21, "thumbnailFormatMpeg4"),
        ],
    });
    ingest(&mut driver, &results.to_string());

    let downloads: Vec<Value> = recorder
        .snapshot()
        .into_iter()
        .filter(|json| json.contains("\"@type\":\"downloadFile\""))
        .map(|json| serde_json::from_str(&json).unwrap())
        .collect();
    let requested: Vec<(i64, i64)> = downloads
        .iter()
        .map(|d| {
            (
                d["file_id"].as_i64().unwrap(),
                d["priority"].as_i64().unwrap(),
            )
        })
        .collect();
    let priority = i64::from(INLINE_RESULTS_DOWNLOAD_PRIORITY);
    // The JPEG still and the MP4 thumbnail loop first, then the clips.
    assert_eq!(
        requested,
        vec![
            (11, priority),
            (21, priority),
            (10, priority),
            (20, priority)
        ]
    );
    // The MP4 thumbnail is a preview clip, never drawn as an image.
    let slot = driver.session.bots.inline_query.as_ref().expect("slot");
    let crate::state::InlineQueryFetch::Loaded { results, .. } = &slot.fetch else {
        panic!("results not loaded");
    };
    assert_eq!(results[1].media.map(|m| m.file_id.0), Some(0));
    assert_eq!(results[1].preview_clip.map(|f| f.0), Some(21));
    let _ = std::fs::remove_dir_all(&dir);
}
