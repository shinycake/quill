use super::storage::{STORAGE_CATEGORY_ORDER, StorageStats, storage_category_label};

/// Batch 6: one selectable row of the local storage list. `file_type` is
/// the `FileType` constructor `optimizeStorage` takes; "Other" folds
/// several types and has none (`None`, not selectable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCategory {
    pub file_type: Option<&'static str>,
    pub label: &'static str,
    pub size: i64,
    pub count: i32,
}

/// Batch 6: the storage categories in TGX order with the file type of
/// each row; empty categories are skipped, unknown types fold into
/// "Other".
pub fn storage_category_entries(stats: &StorageStats) -> Vec<StorageCategory> {
    let mut rows = Vec::new();
    for file_type in STORAGE_CATEGORY_ORDER {
        if let Some(entry) = stats
            .by_file_type
            .iter()
            .find(|e| e.file_type == *file_type)
            && entry.size > 0
        {
            rows.push(StorageCategory {
                file_type: Some(file_type),
                label: storage_category_label(file_type),
                size: entry.size,
                count: entry.count,
            });
        }
    }
    let (size, count) = stats
        .by_file_type
        .iter()
        .filter(|e| !STORAGE_CATEGORY_ORDER.contains(&e.file_type.as_str()))
        .fold((0i64, 0i32), |(size, count), e| {
            (size.saturating_add(e.size), count.saturating_add(e.count))
        });
    if size > 0 || count > 0 {
        rows.push(StorageCategory {
            file_type: None,
            label: "Other",
            size,
            count,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::StorageFileTypeStats;

    #[test]
    fn entries_carry_file_types_and_other_has_none() {
        let stats = StorageStats {
            total_size: 30,
            by_file_type: vec![
                StorageFileTypeStats {
                    file_type: "fileTypePhoto".into(),
                    size: 10,
                    count: 1,
                },
                StorageFileTypeStats {
                    file_type: "fileTypeNotificationSound".into(),
                    size: 5,
                    count: 1,
                },
            ],
            by_chat: Vec::new(),
        };
        let entries = storage_category_entries(&stats);
        assert_eq!(entries[0].file_type, Some("fileTypePhoto"));
        assert_eq!(entries[0].label, "Photos");
        assert_eq!(entries[1].label, "Other");
        assert_eq!(entries[1].file_type, None);
        assert_eq!(entries[1].size, 5);
    }
}
