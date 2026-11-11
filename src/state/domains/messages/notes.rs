//! One-shot notes the message actions leave for the UI to show (a failed
//! resend, a refused link, a rate limit). A queue, so two failures between
//! two drains are both shown.

/// The most notes held when nothing drains them (the window is hidden or
/// locked); the oldest go first.
const KEPT: usize = 16;

#[derive(Debug, Default, Clone)]
pub struct Notes(Vec<String>);

impl Notes {
    pub fn push(&mut self, note: String) {
        self.0.push(note);
        if self.0.len() > KEPT {
            self.0.remove(0);
        }
    }

    /// Every note waiting, oldest first.
    pub fn take_all(&mut self) -> Vec<String> {
        std::mem::take(&mut self.0)
    }

    /// The newest note waiting.
    pub fn last(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Drop every waiting note unread.
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl super::MessagesState {
    /// A one-line outcome of a message action ("message sent").
    pub(crate) fn note_action(&mut self, note: &str) {
        self.message_action_note.push(note.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_notes_between_drains_are_both_kept_in_order() {
        let mut notes = Notes::default();
        notes.push("first".into());
        notes.push("second".into());
        assert_eq!(notes.take_all(), ["first", "second"]);
        assert!(notes.is_empty());
    }

    #[test]
    fn undrained_notes_are_bounded() {
        let mut notes = Notes::default();
        for n in 0..40 {
            notes.push(n.to_string());
        }
        let all = notes.take_all();
        assert_eq!(all.len(), KEPT);
        assert_eq!(all.last().map(String::as_str), Some("39"));
    }
}
