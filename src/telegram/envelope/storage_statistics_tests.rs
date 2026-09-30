use super::*;

/// Phase S2: `getStorageStatistics` answer (schema 1.8.67 lines
/// 9780/9787/9793) — per-chat `by_file_type` entries aggregate into
/// one entry per `fileType` constructor, including `fileTypeSecret`
/// (line 9728).
#[test]
fn storage_statistics_aggregates_by_file_type() {
    let json = r#"{"@type":"storageStatistics","size":7000,"count":3,"by_chat":[{"chat_id":11,"size":5000,"count":2,"by_file_type":[{"file_type":{"@type":"fileTypeSecret"},"size":4000,"count":1},{"file_type":{"@type":"fileTypePhoto"},"size":1000,"count":1}]},{"chat_id":0,"size":2000,"count":1,"by_file_type":[{"file_type":{"@type":"fileTypeSecret"},"size":2000,"count":1}]}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::StorageStatistics {
            total_size,
            by_file_type,
            by_chat,
        } => {
            assert_eq!(total_size, 7000);
            let secret = by_file_type
                .iter()
                .find(|t| t.file_type == "fileTypeSecret")
                .expect("secret category present");
            assert_eq!(secret.size, 6000);
            assert_eq!(secret.count, 2);
            let photo = by_file_type
                .iter()
                .find(|t| t.file_type == "fileTypePhoto")
                .expect("photo category present");
            assert_eq!(photo.size, 1000);
            assert_eq!(photo.count, 1);
            // Slice S4: per-chat rows are kept verbatim.
            assert_eq!(by_chat.len(), 2);
            assert_eq!(by_chat[0].chat_id, 11);
            assert_eq!(by_chat[0].size, 5000);
            assert_eq!(by_chat[0].count, 2);
            assert_eq!(by_chat[1].chat_id, 0);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase S2: every storage-statistics constructor the slice relies
/// on must exist verbatim in the pinned schema (1.8.67).
#[test]
fn schema_pins_storage_statistics_constructors() {
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "fileTypeSecret = FileType;",
        "storageStatisticsByFileType file_type:FileType size:int53 count:int32 = StorageStatisticsByFileType;",
        "storageStatisticsByChat chat_id:int53 size:int53 count:int32 by_file_type:vector<storageStatisticsByFileType> = StorageStatisticsByChat;",
        "storageStatistics size:int53 count:int32 by_chat:vector<storageStatisticsByChat> = StorageStatistics;",
        "getStorageStatistics chat_limit:int32 = StorageStatistics;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
}

/// Phase S2: overlay category rows — TGX order regardless of input
/// order, zero-size categories skipped, `fileTypeSecretThumbnail`
/// and unknown types folded into "Other".
#[test]
fn storage_category_rows_tgx_order_skips_empty_and_folds_other() {
    let entry = |file_type: &str, size: i64, count: i32| StorageFileTypeStats {
        file_type: file_type.to_string(),
        size,
        count,
    };
    let stats = StorageStats {
        total_size: 1000,
        by_chat: Vec::new(),
        by_file_type: vec![
            entry("fileTypeSecret", 200, 2),
            entry("fileTypeBogus", 50, 5),
            entry("fileTypeSecretThumbnail", 30, 3),
            entry("fileTypeVideo", 0, 0),
            entry("fileTypePhoto", 400, 4),
        ],
    };
    let rows = storage_category_rows(&stats);
    assert_eq!(
        rows,
        vec![
            ("Photos", 400, 4),
            ("Secret media and files", 200, 2),
            ("Other", 80, 8),
        ]
    );
}
