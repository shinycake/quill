//! Uploading a notification sound from a file (tdesktop `RingtonesBox`
//! "Upload Sound" and `Api::Ringtones::upload`): the file checks that run
//! before the upload, the server's refusals, and the copy that names them.

use crate::lang::{keys, tr, tr_with, trn};
use crate::message_menu::ToneLimits;
use crate::telegram::envelope::format_size_text;
use std::path::Path;

/// tdesktop's file dialog filter is "Audio files (*.mp3)".
const EXTENSION: &str = "mp3";

/// Why a sound was not added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneProblem {
    /// Bigger than `notification_sound_size_max`.
    TooBig,
    /// Longer than `notification_sound_duration_max`.
    TooLong,
    /// Not an MP3 file, or the server did not accept its type.
    InvalidFile,
    /// The file could not be read, or the upload failed for another reason.
    Unreadable,
}

/// The upload as the session knows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToneUpload {
    #[default]
    Idle,
    Uploading {
        name: String,
    },
    Added {
        sound_id: i64,
    },
    Failed(ToneProblem),
}

impl ToneUpload {
    /// The upload ended and the screen has not reacted to it yet.
    pub fn is_settled(&self) -> bool {
        matches!(self, Self::Added { .. } | Self::Failed(_))
    }
}

/// The name the new row shows: the file name without its extension
/// (`ExtractRingtoneName`).
pub fn display_name(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The checks tdesktop makes before it uploads: the MP3 filter of its file
/// dialog, and `content.size() > ringtones.maxSize()`. The duration limit
/// is the server's to enforce, as in tdesktop.
pub fn check_file(path: &Path, size: u64, limits: ToneLimits) -> Result<(), ToneProblem> {
    let is_mp3 = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION));
    if !is_mp3 {
        return Err(ToneProblem::InvalidFile);
    }
    if i64::try_from(size).map_or(true, |size| size > limits.max_size) {
        return Err(ToneProblem::TooBig);
    }
    Ok(())
}

/// The problem a failed upload names (`RINGTONE_SIZE_TOO_BIG`,
/// `RINGTONE_DURATION_TOO_LONG`, `RINGTONE_MIME_INVALID`).
pub fn problem_for_server_type(server_type: Option<&str>) -> ToneProblem {
    match server_type {
        Some("RINGTONE_SIZE_TOO_BIG") => ToneProblem::TooBig,
        Some("RINGTONE_DURATION_TOO_LONG") => ToneProblem::TooLong,
        Some("RINGTONE_MIME_INVALID") => ToneProblem::InvalidFile,
        _ => ToneProblem::Unreadable,
    }
}

/// tdesktop's wording for `problem`, with the limits filled in.
pub fn problem_message(problem: ToneProblem, limits: ToneLimits) -> String {
    match problem {
        ToneProblem::TooBig => tr_with(
            keys::RINGTONES_ERROR_MAX_SIZE,
            &[("size", &format_size_text(limits.max_size))],
        ),
        ToneProblem::TooLong => tr_with(
            keys::RINGTONES_ERROR_MAX_DURATION,
            &[("duration", &duration_text(limits.max_duration))],
        ),
        ToneProblem::InvalidFile => tr(keys::EDIT_MEDIA_INVALID_FILE).to_string(),
        ToneProblem::Unreadable => tr(keys::Q_TONE_UPLOAD_FAILED).to_string(),
    }
}

fn duration_text(seconds: i32) -> String {
    if seconds >= 60 {
        trn(keys::MINUTES, seconds / 60)
    } else {
        trn(keys::SECONDS, seconds.max(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: ToneLimits = ToneLimits {
        max_size: 100 * 1024,
        max_duration: 5,
        max_count: 100,
    };

    #[test]
    fn an_mp3_within_the_size_limit_passes() {
        assert_eq!(
            check_file(Path::new("/a/Ding.MP3"), 100 * 1024, LIMITS),
            Ok(())
        );
    }

    #[test]
    fn a_file_over_the_limit_is_too_big() {
        assert_eq!(
            check_file(Path::new("/a/ding.mp3"), 100 * 1024 + 1, LIMITS),
            Err(ToneProblem::TooBig)
        );
    }

    #[test]
    fn the_limit_comes_from_the_options() {
        let limits = ToneLimits {
            max_size: 300_000,
            ..LIMITS
        };
        assert_eq!(check_file(Path::new("ding.mp3"), 250_000, limits), Ok(()));
    }

    #[test]
    fn other_types_are_not_audio_files_here() {
        for name in ["ding.wav", "ding", "ding.mp3.txt"] {
            assert_eq!(
                check_file(Path::new(name), 10, LIMITS),
                Err(ToneProblem::InvalidFile),
                "{name}"
            );
        }
    }

    #[test]
    fn server_refusals_map_to_problems() {
        assert_eq!(
            problem_for_server_type(Some("RINGTONE_DURATION_TOO_LONG")),
            ToneProblem::TooLong
        );
        assert_eq!(
            problem_for_server_type(Some("RINGTONE_SIZE_TOO_BIG")),
            ToneProblem::TooBig
        );
        assert_eq!(
            problem_for_server_type(Some("RINGTONE_MIME_INVALID")),
            ToneProblem::InvalidFile
        );
        assert_eq!(problem_for_server_type(None), ToneProblem::Unreadable);
    }

    #[test]
    fn the_row_name_drops_the_extension() {
        assert_eq!(display_name(Path::new("/x/Soft Bell.mp3")), "Soft Bell");
    }

    #[test]
    fn messages_carry_the_limits() {
        assert!(problem_message(ToneProblem::TooBig, LIMITS).contains("100.0 KB"));
        assert!(problem_message(ToneProblem::TooLong, LIMITS).contains("5 seconds"));
    }
}
