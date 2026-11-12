//! The translate dialog's stages: the box, the language chooser and the
//! "Do Not Translate" list are one value, so a page never keeps another
//! page's fields and a transition moves the data it carries.

use super::TranslateSource;
use gpui_kit::component::input::TextareaState;
use gpui_kit::{Entity, Subscription};

/// Where the box's request stands. `Refused`: it could not be sent
/// (offline, a closed client), so the box says it failed instead of
/// showing its placeholder for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TranslateAsk {
    /// Nothing sent from here (a message's translation is read by its
    /// key), or not yet.
    Idle,
    /// A selection's job.
    Job(u64),
    /// The request could not be sent.
    Refused,
}

/// The box: what is translated, where its request stands, and whether a
/// long original is shown whole.
#[derive(Clone, Debug)]
pub(super) struct TranslateBox {
    pub(super) source: TranslateSource,
    pub(super) ask: TranslateAsk,
    pub(super) expanded: bool,
}

impl TranslateBox {
    pub(super) fn new(source: TranslateSource) -> Self {
        Self {
            source,
            ask: TranslateAsk::Idle,
            expanded: false,
        }
    }
}

/// Which page of the translate dialog is showing, with what that page owns.
pub(super) enum TranslateStage {
    /// Original and translation.
    Box(TranslateBox),
    /// Pick the "Translate to" language. `under` is the box the pick goes
    /// back to; a chooser with no box under it closes after a pick.
    Choosing { under: Option<TranslateBox> },
    /// Pick the "Do Not Translate" languages (Settings); `note` is the
    /// last refused toggle.
    SkipList { note: Option<String> },
}

pub(in crate::ui) struct TranslateDialog {
    pub(super) stage: TranslateStage,
    pub(super) to: &'static str,
    pub(super) search: Entity<TextareaState>,
    pub(super) _search_subscription: Subscription,
}

impl TranslateDialog {
    /// The box, while it shows or waits under the chooser.
    pub(super) fn current_box(&self) -> Option<&TranslateBox> {
        match &self.stage {
            TranslateStage::Box(shown) => Some(shown),
            TranslateStage::Choosing { under } => under.as_ref(),
            TranslateStage::SkipList { .. } => None,
        }
    }

    pub(super) fn current_box_mut(&mut self) -> Option<&mut TranslateBox> {
        match &mut self.stage {
            TranslateStage::Box(shown) => Some(shown),
            TranslateStage::Choosing { under } => under.as_mut(),
            TranslateStage::SkipList { .. } => None,
        }
    }

    /// The selection job the box waits on, to drop when the dialog goes.
    pub(super) fn job(&self) -> Option<u64> {
        match self.current_box()?.ask {
            TranslateAsk::Job(job) => Some(job),
            TranslateAsk::Idle | TranslateAsk::Refused => None,
        }
    }

    /// The box's "Language" button: the chooser opens over the box.
    pub(super) fn open_chooser(&mut self) {
        if let TranslateStage::Box(_) = &self.stage {
            let stage =
                std::mem::replace(&mut self.stage, TranslateStage::Choosing { under: None });
            if let TranslateStage::Box(shown) = stage {
                self.stage = TranslateStage::Choosing { under: Some(shown) };
            }
        }
    }

    /// The chooser's Cancel with a box under it: back to the box. `false`
    /// where nothing is under (the dialog closes instead).
    pub(super) fn back_to_box(&mut self) -> bool {
        if let TranslateStage::Choosing { under } = &mut self.stage
            && let Some(shown) = under.take()
        {
            self.stage = TranslateStage::Box(shown);
            return true;
        }
        false
    }

    /// A language was picked: the box under the chooser shows again and
    /// asks for the new language (`true`); with no box under, the dialog is
    /// done (`false`).
    pub(super) fn pick(&mut self, code: &'static str) -> bool {
        self.to = code;
        match &self.stage {
            TranslateStage::Choosing { under: None } => false,
            _ => {
                self.back_to_box();
                true
            }
        }
    }

    pub(super) fn set_skip_note(&mut self, text: Option<String>) {
        if let TranslateStage::SkipList { note } = &mut self.stage {
            *note = text;
        }
    }
}
