//! The Contacts "Import" box's file flow: the state of the chosen `.vcf`
//! file, derived from one value (`VcardImport`). Quill's `parse_vcard`
//! turns the text into cards; this reads the file and keeps what the box
//! shows before anything is sent. Telegram Desktop has no vCard import;
//! see docs/decisions/request-location-vcard-file.md.
#![deny(clippy::indexing_slicing)]

use crate::telegram::requests::{ImportedContact, parse_vcard};
use std::path::Path;

/// A `.vcf` file bigger than this is not a contact list; reading it whole
/// would only stall the box.
pub const VCARD_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Why the chosen file cannot be imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcardError {
    /// The file does not end in `.vcf` or `.vcard`.
    NotVcard,
    Unreadable,
    TooLarge,
    /// A vCard file, but no card has a phone number.
    NoContacts,
}

/// What is about to be imported from one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcardPreview {
    pub file_name: String,
    pub contacts: Vec<ImportedContact>,
    /// Cards without a phone number.
    pub skipped: usize,
    /// Cards past `VCARD_IMPORT_LIMIT`.
    pub truncated: usize,
}

/// The import box: no file yet, a file being read, what the file holds, or
/// why it cannot be used. The box renders from this and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum VcardImport {
    #[default]
    Empty,
    Reading {
        file_name: String,
    },
    Ready(VcardPreview),
    Failed {
        file_name: String,
        error: VcardError,
    },
}

impl VcardImport {
    /// The contacts the Import button sends; empty unless a file is ready.
    pub fn contacts(&self) -> &[ImportedContact] {
        match self {
            VcardImport::Ready(preview) => &preview.contacts,
            _ => &[],
        }
    }

    pub fn can_import(&self) -> bool {
        !self.contacts().is_empty()
    }
}

/// The name shown for `path`.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_vcard_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".vcf") || lower.ends_with(".vcard")
}

/// Read and parse the file at `path`. Blocking: call off the UI thread.
pub fn read_vcard_file(path: &Path) -> VcardImport {
    let file_name = display_name(path);
    let failed = |error| VcardImport::Failed {
        file_name: file_name.clone(),
        error,
    };
    if !is_vcard_name(&file_name) {
        return failed(VcardError::NotVcard);
    }
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > VCARD_MAX_BYTES => return failed(VcardError::TooLarge),
        Ok(_) => {}
        Err(_) => return failed(VcardError::Unreadable),
    }
    match std::fs::read(path) {
        Ok(bytes) => preview_from_bytes(&file_name, &bytes),
        Err(_) => failed(VcardError::Unreadable),
    }
}

/// The preview of a file's bytes (lossy UTF-8: a stray byte in one note
/// must not lose the whole list).
pub fn preview_from_bytes(file_name: &str, bytes: &[u8]) -> VcardImport {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let (contacts, skipped, truncated) = parse_vcard(text);
    if contacts.is_empty() {
        return VcardImport::Failed {
            file_name: file_name.to_string(),
            error: VcardError::NoContacts,
        };
    }
    VcardImport::Ready(VcardPreview {
        file_name: file_name.to_string(),
        contacts,
        skipped,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ada Lovelace\r\nTEL:+15550100\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nFN:No Number\r\nEND:VCARD\r\n";

    #[test]
    fn a_file_with_one_numbered_card_is_ready_and_counts_the_skipped() {
        let state = preview_from_bytes("people.vcf", TWO.as_bytes());
        let VcardImport::Ready(preview) = &state else {
            panic!("expected a ready preview, got {state:?}");
        };
        assert_eq!(preview.contacts.len(), 1);
        assert_eq!(preview.skipped, 1);
        assert_eq!(preview.truncated, 0);
        assert!(state.can_import());
    }

    #[test]
    fn a_file_without_numbers_cannot_be_imported() {
        let state = preview_from_bytes("a.vcf", b"BEGIN:VCARD\nFN:X\nEND:VCARD\n");
        assert_eq!(
            state,
            VcardImport::Failed {
                file_name: "a.vcf".into(),
                error: VcardError::NoContacts
            }
        );
        assert!(!state.can_import());
        assert!(!VcardImport::Empty.can_import());
    }

    #[test]
    fn a_byte_order_mark_and_stray_bytes_do_not_lose_the_list() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(TWO.as_bytes());
        bytes.push(0xFF);
        assert!(preview_from_bytes("a.vcf", &bytes).can_import());
    }

    #[test]
    fn other_file_types_are_refused_by_name() {
        let state = read_vcard_file(Path::new("/nowhere/notes.txt"));
        assert!(matches!(
            state,
            VcardImport::Failed {
                error: VcardError::NotVcard,
                ..
            }
        ));
        let missing = read_vcard_file(Path::new("/nowhere/people.VCF"));
        assert!(matches!(
            missing,
            VcardImport::Failed {
                error: VcardError::Unreadable,
                ..
            }
        ));
    }

    #[test]
    fn a_real_file_is_read() {
        let dir = std::env::temp_dir().join(format!("quill-vcf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("people.vcf");
        std::fs::write(&path, TWO).expect("write");
        let state = read_vcard_file(&path);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(state.contacts().len(), 1);
    }
}
