//! The status note: one line of feedback ("Link copied", "Couldn't send")
//! with a kind chosen where the action reports it. Whether a note shows as
//! a toast, and for how long, follows the kind, never the words, so every
//! interface language behaves like English.

use std::time::Duration;

/// How a note is presented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum NoteKind {
    /// A confirmation or restriction the screen does not show by itself
    /// (copied, saved, Premium needed): a toast for a few seconds.
    #[default]
    Info,
    /// A failure: a toast that stays longer.
    Error,
    /// Progress chatter the screen already shows (the bubble's sending
    /// clock): kept as the current note, never toasted.
    Silent,
}

/// The current status line: text plus how it is presented.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StatusNote {
    text: String,
    kind: NoteKind,
}

impl StatusNote {
    pub(crate) fn new(kind: NoteKind, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }

    pub(crate) fn info(text: impl Into<String>) -> Self {
        Self::new(NoteKind::Info, text)
    }

    pub(crate) fn error(text: impl Into<String>) -> Self {
        Self::new(NoteKind::Error, text)
    }

    pub(crate) fn silent(text: impl Into<String>) -> Self {
        Self::new(NoteKind::Silent, text)
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn kind(&self) -> NoteKind {
        self.kind
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Whether the live window floats this note as a toast.
    pub(crate) fn is_toast(&self) -> bool {
        !self.text.is_empty() && self.kind != NoteKind::Silent
    }

    /// How long the toast stays: failures longer than confirmations.
    pub(crate) fn toast_duration(&self) -> Duration {
        Duration::from_millis(match self.kind {
            NoteKind::Error => 6000,
            NoteKind::Info | NoteKind::Silent => 3000,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{NoteKind, StatusNote};
    use std::time::Duration;

    #[test]
    fn the_kind_decides_the_toast_whatever_the_language() {
        let german = StatusNote::info("Text in die Zwischenablage kopiert");
        assert!(german.is_toast());
        assert_eq!(german.toast_duration(), Duration::from_secs(3));
        let german_error = StatusNote::error("Senden fehlgeschlagen");
        assert!(german_error.is_toast());
        assert_eq!(german_error.toast_duration(), Duration::from_secs(6));
    }

    #[test]
    fn silent_and_empty_notes_never_toast_even_with_failure_words() {
        assert!(!StatusNote::silent("sending failed… retrying").is_toast());
        assert!(!StatusNote::info("").is_toast());
        assert!(!StatusNote::default().is_toast());
        assert_eq!(StatusNote::default().kind(), NoteKind::Info);
    }
}
