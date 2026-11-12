//! Where the forward a window builds stands (tdesktop `Data::ForwardDraft`
//! with the history's multi-select, `ShowForwardMessagesBox` and the
//! field's forward bar), as one value.
//!
//! It was three fields kept in step by about ten writers: the draft, the
//! chat the forward bar waits in and whether the share box is open. A
//! writer that cleared one and not the others left a bar with no draft, a
//! box over nothing, or a forward bar into the selection's own chat that
//! kept selection mode on in every topic of it.
//!
//! "Reply in Another Chat"'s chat chooser draws the same box but is the
//! reply bar's panel (`ReplyPanel`, held by `ChatUi`): it goes with the
//! reply, which this stage knows nothing about. Opening one ends the other
//! (`QuillApp::open_forward_picker`, `QuillApp::show_reply_panel`).
#![deny(clippy::indexing_slicing)]

use quill::composer::ForwardDraft;
use quill::ids::{ChatId, MessageId};

use super::mount::Mount;

/// A viewer video's message and second the share box was opened at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShareAt {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) secs: i32,
}

/// The forward being built in one window.
#[derive(Debug, Default)]
pub(crate) enum ForwardStage {
    /// Nothing picked.
    #[default]
    None,
    /// Messages ticked in the conversation they were picked in: selection
    /// mode there.
    Selecting(ForwardDraft),
    /// The share box picks where the messages go. `bar` is the chat whose
    /// forward bar the box was opened over ("Change recipient"), which
    /// leaving the box returns to; `None`: the box was opened from a
    /// selection, which leaving it returns to.
    ///
    /// `mount` is the box's own opening stage: its search field takes focus
    /// on the first draw after the dialog has mounted. `share_at` is the
    /// viewer position ("Share at 1:23") the box was opened at, which
    /// lasts as long as the box.
    Picking {
        draft: ForwardDraft,
        bar: Option<ChatId>,
        mount: Mount,
        share_at: Option<ShareAt>,
    },
    /// One destination chosen: the forward bar waits above `dest`'s
    /// composer, and Send forwards (the typed comment first). No selection
    /// mode anywhere: the bar is the forward now.
    Bar { draft: ForwardDraft, dest: ChatId },
}

impl ForwardStage {
    /// The stored draft, whatever the stage (deleted ids included; read
    /// `QuillApp::forward_draft` for anything shown or done).
    pub(crate) fn draft(&self) -> Option<&ForwardDraft> {
        match self {
            Self::None => None,
            Self::Selecting(draft) | Self::Picking { draft, .. } | Self::Bar { draft, .. } => {
                Some(draft)
            }
        }
    }

    /// The draft's options (send as copy, drop captions) or ids, the stage
    /// unchanged.
    pub(crate) fn draft_mut(&mut self) -> Option<&mut ForwardDraft> {
        match self {
            Self::None => None,
            Self::Selecting(draft) | Self::Picking { draft, .. } | Self::Bar { draft, .. } => {
                Some(draft)
            }
        }
    }

    /// The selection, for a change of its ids: a forward bar waiting goes
    /// back to selection mode first, since the user is picking again.
    pub(crate) fn selection_mut(&mut self) -> Option<&mut ForwardDraft> {
        *self = match std::mem::take(self) {
            Self::Bar { draft, .. } => Self::Selecting(draft),
            other => other,
        };
        self.draft_mut()
    }

    /// A forward is being built.
    pub(crate) fn is_some(&self) -> bool {
        self.draft().is_some()
    }

    /// The first draw of an opened box asks: true once
    /// per opening, so its search field takes focus after the dialog has
    /// mounted. The opening stage is the box's own; nothing sets it by hand.
    pub(crate) fn take_first_draw(&mut self) -> bool {
        match self {
            Self::Picking { mount, .. } => mount.take_first_draw(),
            _ => false,
        }
    }

    /// Record the viewer position the open box was opened at (nothing
    /// when no box is open).
    pub(crate) fn set_share_at(&mut self, at: ShareAt) {
        if let Self::Picking { share_at, .. } = self {
            *share_at = Some(at);
        }
    }

    /// The position the open box was opened at for this message, taken:
    /// the link carries it once.
    pub(crate) fn take_share_at(&mut self, chat_id: ChatId, message_id: MessageId) -> Option<i32> {
        let Self::Picking { share_at, .. } = self else {
            return None;
        };
        share_at
            .take_if(|at| at.chat_id == chat_id && at.message_id == message_id)
            .map(|at| at.secs)
    }

    /// The share box is open.
    pub(crate) fn picker_open(&self) -> bool {
        matches!(self, Self::Picking { .. })
    }

    /// The chat the forward bar is for, shown or under the box.
    pub(crate) fn bar_dest(&self) -> Option<ChatId> {
        match self {
            Self::Bar { dest, .. } => Some(*dest),
            Self::Picking { bar, .. } => *bar,
            Self::None | Self::Selecting(_) => None,
        }
    }

    /// The forward bar showing (not under the box), and its chat.
    pub(crate) fn bar_shown(&self) -> Option<ChatId> {
        match self {
            Self::Bar { dest, .. } => Some(*dest),
            _ => None,
        }
    }

    /// The draft is a selection: selection mode in the conversation it
    /// was picked in (the box may be open over it).
    pub(crate) fn selection(&self) -> Option<&ForwardDraft> {
        match self {
            Self::Selecting(draft)
            | Self::Picking {
                draft, bar: None, ..
            } => Some(draft),
            _ => None,
        }
    }

    /// The forward goes; the draft that was there is handed back.
    pub(crate) fn take_draft(&mut self) -> Option<ForwardDraft> {
        match std::mem::take(self) {
            Self::None => None,
            Self::Selecting(draft) | Self::Picking { draft, .. } | Self::Bar { draft, .. } => {
                Some(draft)
            }
        }
    }

    /// Open the box over what is there. `false` with nothing to forward.
    pub(crate) fn open_picker(&mut self) -> bool {
        if !self.is_some() {
            return false;
        }
        *self = match std::mem::take(self) {
            Self::None => return false,
            Self::Selecting(draft) => Self::Picking {
                draft,
                bar: None,
                mount: Mount::Mounting,
                share_at: None,
            },
            Self::Bar { draft, dest } => Self::Picking {
                draft,
                bar: Some(dest),
                mount: Mount::Mounting,
                share_at: None,
            },
            picking @ Self::Picking { .. } => picking,
        };
        true
    }

    /// Leave the box: back to the selection or the bar it was opened over.
    pub(crate) fn close_picker(&mut self) {
        *self = match std::mem::take(self) {
            Self::Picking {
                draft, bar: None, ..
            } => Self::Selecting(draft),
            Self::Picking {
                draft,
                bar: Some(dest),
                ..
            } => Self::Bar { draft, dest },
            other => other,
        };
    }

    /// The forward bar goes (the view left its chat): a selection stays,
    /// one the box was opened over included.
    pub(crate) fn drop_bar(&mut self) {
        *self = match std::mem::take(self) {
            Self::Bar { draft, .. } => Self::Selecting(draft),
            Self::Picking {
                draft,
                mount,
                share_at,
                ..
            } => Self::Picking {
                draft,
                bar: None,
                mount,
                share_at,
            },
            other => other,
        };
    }

    /// A stage whose ids were all unticked is no forward.
    pub(crate) fn drop_if_empty(&mut self) {
        if !self.draft().is_some_and(ForwardDraft::is_empty) {
            return;
        }
        *self = Self::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill::ids::{MessageId, TopicId};

    fn draft() -> ForwardDraft {
        ForwardDraft::from_message(ChatId(1), TopicId::None, MessageId(5), false)
            .unwrap_or_else(|| panic!("a sent message"))
    }

    #[test]
    fn the_box_returns_to_what_it_was_opened_over() {
        let mut stage = ForwardStage::Selecting(draft());
        assert!(stage.open_picker());
        assert!(stage.picker_open() && stage.selection().is_some());
        stage.close_picker();
        assert!(matches!(stage, ForwardStage::Selecting(_)));

        let mut stage = ForwardStage::Bar {
            draft: draft(),
            dest: ChatId(2),
        };
        assert!(stage.open_picker());
        assert_eq!(stage.bar_dest(), Some(ChatId(2)));
        assert_eq!(stage.bar_shown(), None, "under the box");
        assert!(stage.selection().is_none(), "a bar is no selection");
        stage.close_picker();
        assert_eq!(stage.bar_shown(), Some(ChatId(2)));

        assert!(!ForwardStage::None.open_picker());
    }

    #[test]
    fn picking_again_turns_a_bar_back_into_a_selection() {
        let mut stage = ForwardStage::Bar {
            draft: draft(),
            dest: ChatId(1),
        };
        assert!(stage.selection().is_none());
        if let Some(draft) = stage.selection_mut() {
            draft.toggle(ChatId(1), TopicId::None, MessageId(6), false);
        }
        assert_eq!(stage.selection().map(ForwardDraft::count), Some(2));
        assert_eq!(stage.bar_dest(), None);
    }

    #[test]
    fn share_at_lasts_as_long_as_the_box_and_is_taken_once() {
        let at = ShareAt {
            chat_id: ChatId(1),
            message_id: MessageId(7),
            secs: 83,
        };
        let mut stage = ForwardStage::Selecting(draft());
        stage.set_share_at(at);
        assert_eq!(
            stage.take_share_at(ChatId(1), MessageId(7)),
            None,
            "no box is open"
        );
        assert!(stage.open_picker());
        stage.set_share_at(at);
        assert_eq!(stage.take_share_at(ChatId(1), MessageId(8)), None);
        assert_eq!(stage.take_share_at(ChatId(1), MessageId(7)), Some(83));
        assert_eq!(stage.take_share_at(ChatId(1), MessageId(7)), None, "once");
        stage.set_share_at(at);
        stage.close_picker();
        assert!(stage.open_picker());
        assert_eq!(
            stage.take_share_at(ChatId(1), MessageId(7)),
            None,
            "closing the box dropped it"
        );
    }

    #[test]
    fn leaving_the_bars_chat_keeps_the_selection_only() {
        let mut stage = ForwardStage::Picking {
            draft: draft(),
            bar: Some(ChatId(2)),
            mount: Mount::Shown,
            share_at: None,
        };
        stage.drop_bar();
        assert!(stage.picker_open());
        assert_eq!(stage.bar_dest(), None);
        stage.close_picker();
        assert!(matches!(stage, ForwardStage::Selecting(_)));
    }
}
