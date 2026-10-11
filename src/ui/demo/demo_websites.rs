//! Methods moved out of `demo.rs` to keep files under 1000 lines.

use super::*;

/// Slice A4: `getConnectedWebsites` fixture for the web-sessions
/// screenshot demo — three connected websites (injected, no live
/// Telegram).
pub(in crate::ui) fn demo_websites() -> Vec<ParsedWebsite> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i32)
        .unwrap_or(0);
    vec![
        ParsedWebsite {
            id: 1000000001,
            domain_name: "fragment.com".into(),
            bot_user_id: 999888111,
            browser: "Chrome".into(),
            platform: "Web".into(),
            log_in_date: now - 90 * 86400,
            last_active_date: now - 1800,
            ip_address: "203.0.113.42".into(),
            location: "Austin, United States".into(),
        },
        ParsedWebsite {
            id: 1000000002,
            domain_name: "t.me".into(),
            bot_user_id: 777666555,
            browser: "Safari".into(),
            platform: "Web".into(),
            log_in_date: now - 40 * 86400,
            last_active_date: now - 2 * 86400,
            ip_address: "198.51.100.7".into(),
            location: "Dallas, United States".into(),
        },
        ParsedWebsite {
            id: 1000000003,
            domain_name: "wallet.bot".into(),
            bot_user_id: 444333222,
            browser: "Firefox".into(),
            platform: "Web".into(),
            log_in_date: now - 10 * 86400,
            last_active_date: now - 86400,
            ip_address: "192.0.2.19".into(),
            location: "Unknown".into(),
        },
    ]
}

pub(in crate::ui) fn demo_storage_stats() -> StorageStats {
    StorageStats {
        total_size: 1_234_567_890,
        by_chat: vec![
            StorageChatStats {
                chat_id: 11,
                size: 600_000_000,
                count: 800,
            },
            StorageChatStats {
                chat_id: 13,
                size: 400_000_000,
                count: 300,
            },
            StorageChatStats {
                chat_id: 12,
                size: 150_000_000,
                count: 143,
            },
        ],
        by_file_type: vec![
            StorageFileTypeStats {
                file_type: "fileTypePhoto".to_string(),
                size: 800_000_000,
                count: 1200,
            },
            StorageFileTypeStats {
                file_type: "fileTypeVideo".to_string(),
                size: 300_000_000,
                count: 45,
            },
            StorageFileTypeStats {
                file_type: "fileTypeSecret".to_string(),
                size: 96_000_000,
                count: 210,
            },
            StorageFileTypeStats {
                file_type: "fileTypeDocument".to_string(),
                size: 30_000_000,
                count: 88,
            },
            StorageFileTypeStats {
                file_type: "fileTypeSticker".to_string(),
                size: 8_000_000,
                count: 640,
            },
        ],
    }
}
