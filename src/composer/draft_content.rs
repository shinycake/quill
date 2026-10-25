//! What a composer draft save carries.
use crate::rich::RichDraft;

/// The composer's content for a draft slot: message markup from the
/// field, or a rich draft shown in place of the field (tdesktop keeps a
/// cloud draft with either `textWithTags` or `richMessage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftContent {
    Text(String),
    Rich(RichDraft),
}

impl DraftContent {
    /// The content worth saving, or `None` to clear the slot: blank text
    /// clears it unless `keeps_draft` (a reply or post terms still make a
    /// draft).
    pub fn to_store(self, keeps_draft: bool) -> Option<Self> {
        match self {
            Self::Text(text) => super::draft_text_to_store(&text, keeps_draft)
                .is_some()
                .then_some(Self::Text(text)),
            rich @ Self::Rich(_) => Some(rich),
        }
    }

    /// The text a [`ChatDraft`](crate::telegram::envelope::ChatDraft)
    /// stores: empty for a rich draft.
    pub fn text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Rich(_) => "",
        }
    }

    pub fn rich(&self) -> Option<&RichDraft> {
        match self {
            Self::Text(_) => None,
            Self::Rich(rich) => Some(rich),
        }
    }
}
