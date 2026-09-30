//! Full account data export (`parity:platform-data-export`) — Telegram
//! Desktop's "Export Telegram data".
//!
//! TDLib has no account-export constructor, so the export is assembled
//! client-side: the account holder (`updateUser` for self), the contacts
//! (`getContacts`, see `connect::contacts`), every chat's history paged
//! through the per-chat [`crate::chat_export`] machinery (same JSON format,
//! one file per chat), and message media downloaded through the existing
//! `downloadFile` path and copied into the bundle.
//!
//! [`DataExportState`] lives on `Session`; the driver pumps it from the
//! app's poll loop (`pump_data_export`), so the UI thread never blocks.
//! The export is cancellable by dropping the state.

use crate::chat_export::project_message;
use crate::ids::{ChatId, FileId};
use crate::state::Session;
use crate::telegram::envelope::ParsedMessage;
use serde::Serialize;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// What the export includes. Account info always lands in `account.json`;
/// the toggles gate the expensive parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataExportOptions {
    /// Every (non-secret) chat's message history, one JSON file per chat.
    pub chats: bool,
    /// The contact list.
    pub contacts: bool,
    /// Message media files (photos, videos, documents, …).
    pub media: bool,
}

impl Default for DataExportOptions {
    fn default() -> Self {
        Self {
            chats: true,
            contacts: true,
            media: true,
        }
    }
}

/// One media file queued for the bundle.
#[derive(Debug, Clone)]
pub struct MediaJob {
    pub file_id: i32,
    pub chat_id: i64,
    pub message_id: i64,
    /// File name used inside the bundle's `media/` directory.
    pub file_name: String,
}

/// One row of `media_manifest.json`: which message a bundled file came from.
#[derive(Debug, Clone, Serialize)]
pub struct MediaManifestEntry {
    pub chat_id: i64,
    pub message_id: i64,
    pub file: String,
}

/// One exported contact row.
#[derive(Debug, Clone, Serialize)]
pub struct ExportedContact {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub phone_number: String,
}

/// In-progress account export, held on `Session::data_export`.
pub struct DataExportState {
    pub options: DataExportOptions,
    /// Bundle root (`<dest>/quill-data-export-<unix>/`).
    pub dir: PathBuf,
    /// Chats still to page, popped from the end.
    pub queue: Vec<(ChatId, String)>,
    pub total_chats: usize,
    pub exported_chats: usize,
    pub total_messages: usize,
    /// Secret chats are skipped (out of scope) — counted for the summary.
    pub skipped_secret_chats: usize,
    /// Media files awaiting download/copy.
    pub media_jobs: Vec<MediaJob>,
    /// File ids already queued (a file shared by several messages is
    /// exported once).
    media_seen: HashSet<i32>,
    pub media_done: usize,
    pub media_failed: usize,
    pub media_manifest: Vec<MediaManifestEntry>,
    /// The contacts phase ran (loaded or gave up with `contacts_note`).
    pub contacts_fetched: bool,
    pub contacts_note: Option<String>,
    /// Set when the export cannot continue; the UI surfaces and clears it.
    pub failed: Option<String>,
    /// Set when the bundle (including `account.json`) is fully written.
    pub finished: bool,
    /// The completion/failure note was already surfaced once.
    pub note_surfaced: bool,
}

impl DataExportState {
    pub fn new(options: DataExportOptions, dir: PathBuf) -> Self {
        Self {
            options,
            dir,
            queue: Vec::new(),
            total_chats: 0,
            exported_chats: 0,
            total_messages: 0,
            skipped_secret_chats: 0,
            media_jobs: Vec::new(),
            media_seen: HashSet::new(),
            media_done: 0,
            media_failed: 0,
            media_manifest: Vec::new(),
            contacts_fetched: false,
            contacts_note: None,
            failed: None,
            finished: false,
            note_surfaced: false,
        }
    }

    pub fn settled(&self) -> bool {
        self.failed.is_some() || self.finished
    }

    /// 0.0..=1.0 progress across chats and media jobs. Chats dominate the
    /// work; media is the tail.
    pub fn progress_fraction(&self) -> f32 {
        let chats_total = self.exported_chats + self.queue.len();
        let chats_frac = if chats_total == 0 {
            1.0
        } else {
            self.exported_chats as f32 / chats_total as f32
        };
        let media_total = self.media_done + self.media_failed + self.media_jobs.len();
        if media_total == 0 {
            chats_frac
        } else {
            let media_frac = (self.media_done + self.media_failed) as f32 / media_total as f32;
            0.8 * chats_frac + 0.2 * media_frac
        }
    }

    /// Short phase label for the progress dialog. `chat_active` is whether
    /// the driver is currently paging a chat (the in-flight chat has
    /// already left `queue`, so the state alone can't tell).
    pub fn phase_label(&self, chat_active: bool) -> &'static str {
        if self.options.contacts && !self.contacts_fetched {
            "Loading contacts…"
        } else if !self.media_jobs.is_empty() {
            "Downloading media…"
        } else if !self.queue.is_empty() || chat_active {
            "Exporting chats…"
        } else {
            "Finishing…"
        }
    }

    /// Seed the chat queue (alphabetical by title); secret chats were
    /// filtered out by the caller and are reported via `skipped_secret`.
    pub fn set_queue(&mut self, mut chats: Vec<(ChatId, String)>, skipped_secret: usize) {
        chats.sort_by(|a, b| a.1.cmp(&b.1));
        chats.reverse();
        self.total_chats = chats.len();
        self.queue = chats;
        self.skipped_secret_chats = skipped_secret;
    }

    /// Record a finished chat page-run: the per-chat JSON is already on
    /// disk (written by the shared `chat_export` path).
    pub fn note_chat_done(&mut self, message_count: usize) {
        self.exported_chats += 1;
        self.total_messages += message_count;
    }

    /// Queue a message's media files for the bundle. Files that are
    /// already local are copied without a download; the pump dedups by
    /// file id.
    pub fn enqueue_media_from(&mut self, message: &ParsedMessage, chat_id: i64) {
        if !self.options.media {
            return;
        }
        let label = project_message(message).media;
        for file in &message.files {
            if file.id.0 == 0 || !self.media_seen.insert(file.id.0) {
                continue;
            }
            self.media_jobs.push(MediaJob {
                file_id: file.id.0,
                chat_id,
                message_id: message.id.0,
                file_name: media_file_name(file.id, label.as_deref()),
            });
        }
    }

    /// Queue the account holder's profile photo (called once at start).
    pub fn enqueue_profile_photo(&mut self, file_id: i32) {
        if self.options.media && file_id != 0 && self.media_seen.insert(file_id) {
            self.media_jobs.push(MediaJob {
                file_id,
                chat_id: 0,
                message_id: 0,
                file_name: format!("profile-photo-{file_id}"),
            });
        }
    }

    /// Destination path for a finished media job inside the bundle.
    pub(crate) fn media_dest(&self, job: &MediaJob) -> PathBuf {
        self.dir.join("media").join(&job.file_name)
    }

    /// Write `account.json`, `contacts.json` and `media_manifest.json`
    /// into the bundle. Per-chat JSONs are written by the shared
    /// `chat_export` path as each chat finishes.
    pub fn finalize(&self, session: &Session) -> Result<(), String> {
        let now = unix_now();
        let user = session
            .my_user_id
            .and_then(|id| session.users.get(&id))
            .map(|u| {
                serde_json::json!({
                    "id": u.id,
                    "first_name": u.first_name,
                    "last_name": u.last_name,
                    "username": u.username,
                    "phone_number": u.phone_number,
                })
            })
            .unwrap_or(serde_json::Value::Null);
        let mut notes = Vec::new();
        if self.skipped_secret_chats > 0 {
            notes.push(format!(
                "{} secret chat(s) were skipped: secret-chat history export is out of scope",
                self.skipped_secret_chats
            ));
        }
        if user.is_null() {
            notes.push(
                "account holder details were unavailable (no cached user object)".to_string(),
            );
        }
        if let Some(note) = &self.contacts_note {
            notes.push(note.clone());
        }
        let account = serde_json::json!({
            "app": "quill",
            "exported_at": now,
            "user": user,
            "stats": {
                "chats": self.exported_chats,
                "messages": self.total_messages,
                "contacts": session.contacts.as_ref().map(|c| c.len()).unwrap_or(0),
                "media_files": self.media_done,
                "media_failed": self.media_failed,
                "secret_chats_skipped": self.skipped_secret_chats,
            },
            "notes": notes,
        });
        write_json(&self.dir.join("account.json"), &account)
            .map_err(|err| format!("could not write account.json: {err}"))?;

        if self.options.contacts {
            let contacts: Vec<ExportedContact> = session
                .contacts
                .as_ref()
                .map(|ids| {
                    ids.iter()
                        .map(|id| {
                            let name = session
                                .users
                                .get(id)
                                .map(|u| {
                                    format!("{} {}", u.first_name, u.last_name)
                                        .trim()
                                        .to_string()
                                })
                                .unwrap_or_default();
                            let (username, phone) = session
                                .users
                                .get(id)
                                .map(|u| (u.username.clone(), u.phone_number.clone()))
                                .unwrap_or_default();
                            ExportedContact {
                                id: *id,
                                name,
                                username,
                                phone_number: phone,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let payload = serde_json::json!({
                "app": "quill",
                "exported_at": now,
                "note": self.contacts_note,
                "contacts": contacts,
            });
            write_json(&self.dir.join("contacts.json"), &payload)
                .map_err(|err| format!("could not write contacts.json: {err}"))?;
        }

        if self.options.media {
            let payload = serde_json::json!({
                "app": "quill",
                "exported_at": now,
                "files": self.media_manifest,
            });
            write_json(&self.dir.join("media_manifest.json"), &payload)
                .map_err(|err| format!("could not write media_manifest.json: {err}"))?;
        }
        Ok(())
    }
}

/// Create the bundle directory `<parent>/quill-data-export-<unix>/`
/// (numeric suffix on collision, like `chat_export::write_export`).
pub fn new_bundle_dir(parent: &Path) -> io::Result<PathBuf> {
    let now = unix_now();
    let mut dir = parent.join(format!("quill-data-export-{now}"));
    for suffix in 1.. {
        if !dir.exists() {
            break;
        }
        dir = parent.join(format!("quill-data-export-{now}-{suffix}"));
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::create_dir_all(dir.join("chats"))?;
    std::fs::create_dir_all(dir.join("media"))?;
    Ok(dir)
}

/// Bundle file name for a media job: the document's original name when
/// the projection carried one, else `file-<id>`, sanitized.
fn media_file_name(file_id: FileId, media_label: Option<&str>) -> String {
    let stem = match media_label {
        Some(label) => label
            .strip_prefix("document: ")
            .map(|name| name.to_string())
            .unwrap_or_else(|| format!("file-{}", file_id.0)),
        None => format!("file-{}", file_id.0),
    };
    sanitize_file_name(&stem, &format!("file-{}", file_id.0))
}

/// Keep alphanumerics plus a small safe set; everything else becomes `_`.
/// Falls back to `fallback` when nothing survives.
pub fn sanitize_file_name(name: &str, fallback: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let safe = safe.trim();
    if safe.is_empty() {
        fallback.to_string()
    } else {
        safe.to_string()
    }
}

fn write_json(path: &Path, value: &serde_json::Value) -> io::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(value)?)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MessageId;
    use crate::telegram::envelope::{LocalFileState, MessageContent, ParsedFile, TextContent};

    fn file(id: i32) -> ParsedFile {
        ParsedFile {
            id: FileId(id),
            size: 10,
            expected_size: 10,
            local: LocalFileState {
                path: String::new(),
                can_be_downloaded: true,
                is_downloading_active: false,
                is_downloading_completed: false,
                downloaded_size: 0,
            },
        }
    }

    fn message(id: i64, files: Vec<ParsedFile>) -> ParsedMessage {
        ParsedMessage {
            id: MessageId(id),
            chat_id: ChatId(7),
            date: 1_700_000_000,
            is_outgoing: false,
            is_pinned: false,
            topic_id: None,
            media_album_id: 0,
            author_signature: None,
            scheduling_state: None,
            can_retry: false,
            content: MessageContent::Text(TextContent {
                text: String::new(),
                entities: Vec::new(),
                link_preview: None,
            }),
            ephemeral: None,
            files,
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
        }
    }

    #[test]
    fn media_jobs_dedup_by_file_id() {
        let dir = std::env::temp_dir();
        let mut dx = DataExportState::new(DataExportOptions::default(), dir);
        dx.enqueue_media_from(&message(1, vec![file(11), file(12)]), 7);
        // Same file on another message: queued once.
        dx.enqueue_media_from(&message(2, vec![file(11)]), 7);
        assert_eq!(dx.media_jobs.len(), 2);
        assert_eq!(dx.media_jobs[0].file_name, "file-11");
    }

    #[test]
    fn document_keeps_original_name() {
        let name = media_file_name(FileId(5), Some("document: report.pdf"));
        assert_eq!(name, "report.pdf");
        let name = media_file_name(FileId(5), Some("photo"));
        assert_eq!(name, "file-5");
    }

    #[test]
    fn sanitize_drops_unsafe_chars() {
        assert_eq!(sanitize_file_name("a/b\\c:d", "fb"), "a_b_c_d");
        assert_eq!(sanitize_file_name("///", "fb"), "___");
        assert_eq!(sanitize_file_name("", "fb"), "fb");
        assert_eq!(sanitize_file_name("ok name-1.txt", "fb"), "ok name-1.txt");
    }

    #[test]
    fn media_opt_out_skips_queueing() {
        let dir = std::env::temp_dir();
        let mut dx = DataExportState::new(
            DataExportOptions {
                media: false,
                ..DataExportOptions::default()
            },
            dir,
        );
        dx.enqueue_media_from(&message(1, vec![file(11)]), 7);
        assert!(dx.media_jobs.is_empty());
    }

    #[test]
    fn new_bundle_dir_creates_unique_dirs() {
        let parent = std::env::temp_dir().join(format!(
            "quill-data-export-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let a = new_bundle_dir(&parent).unwrap();
        let b = new_bundle_dir(&parent).unwrap();
        assert_ne!(a, b);
        assert!(a.join("chats").is_dir());
        assert!(a.join("media").is_dir());
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn queue_is_seeded_alphabetically() {
        let dir = std::env::temp_dir();
        let mut dx = DataExportState::new(DataExportOptions::default(), dir);
        dx.set_queue(
            vec![(ChatId(2), "Zulu".into()), (ChatId(1), "Alpha".into())],
            3,
        );
        assert_eq!(dx.total_chats, 2);
        assert_eq!(dx.skipped_secret_chats, 3);
        // Popped from the end → alphabetical order.
        assert_eq!(dx.queue.pop().unwrap().1, "Alpha");
        assert_eq!(dx.queue.pop().unwrap().1, "Zulu");
    }
}
