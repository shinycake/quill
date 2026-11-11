//! The scheduled-messages box of a chat and the messages ticked in it: one
//! value, so the ticks cannot outlive the box or be read while it is shut.
//! It lives in the chat's `ChatUi`, so a chat change drops it with the chat.

use quill::ids::MessageId;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) enum ScheduledBox {
    #[default]
    Closed,
    Open {
        selected: Vec<MessageId>,
    },
    /// The box is hidden while the schedule picker asks for a new time
    /// (Reschedule); the ticked messages wait for it or for Cancel.
    Rescheduling {
        selected: Vec<MessageId>,
    },
}

impl ScheduledBox {
    /// The box is on screen.
    pub(super) fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }

    /// The ticked messages (none while closed).
    pub(super) fn selected(&self) -> &[MessageId] {
        match self {
            Self::Closed => &[],
            Self::Open { selected } | Self::Rescheduling { selected } => selected,
        }
    }

    /// The box opens with nothing ticked.
    pub(super) fn open_fresh(&mut self) {
        *self = Self::Open {
            selected: Vec::new(),
        };
    }

    /// The box comes back after the picker: ticks that waited for it stay.
    pub(super) fn resume(&mut self) {
        *self = match std::mem::take(self) {
            Self::Rescheduling { selected } | Self::Open { selected } => Self::Open { selected },
            Self::Closed => Self::Open {
                selected: Vec::new(),
            },
        };
    }

    /// The box hides for the schedule picker; the ticks wait.
    pub(super) fn hide_for_picker(&mut self) {
        *self = match std::mem::take(self) {
            Self::Open { selected } | Self::Rescheduling { selected } => {
                Self::Rescheduling { selected }
            }
            Self::Closed => Self::Closed,
        };
    }

    /// Closed (Esc, the X, a button that leaves it): the ticks go.
    pub(super) fn close(&mut self) {
        *self = Self::Closed;
    }

    /// The picker took the ticked messages to reschedule them.
    pub(super) fn take_selected(&mut self) -> Vec<MessageId> {
        match std::mem::take(self) {
            Self::Open { selected } | Self::Rescheduling { selected } => selected,
            Self::Closed => Vec::new(),
        }
    }

    pub(super) fn clear_selection(&mut self) {
        if let Self::Open { selected } | Self::Rescheduling { selected } = self {
            selected.clear();
        }
    }

    /// A tick of `id` flipped (only while the box is open).
    pub(super) fn toggle(&mut self, id: MessageId) {
        if let Self::Open { selected } = self {
            quill::selection_pin::toggle_id(selected, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScheduledBox;
    use quill::ids::MessageId;

    #[test]
    fn ticks_wait_through_the_picker_and_go_when_the_box_closes() {
        let mut b = ScheduledBox::default();
        assert!(b.selected().is_empty());
        b.toggle(MessageId(1));
        assert!(b.selected().is_empty(), "nothing to tick while closed");
        b.open_fresh();
        b.toggle(MessageId(1));
        b.toggle(MessageId(2));
        b.toggle(MessageId(1));
        assert_eq!(b.selected(), &[MessageId(2)]);
        b.hide_for_picker();
        assert!(!b.is_open());
        assert_eq!(b.selected(), &[MessageId(2)], "kept for the new time");
        b.resume();
        assert!(b.is_open());
        assert_eq!(
            b.selected(),
            &[MessageId(2)],
            "Cancel brings the ticks back"
        );
        b.close();
        assert!(b.selected().is_empty(), "closing drops the ticks");
        b.resume();
        assert!(b.selected().is_empty());
    }

    #[test]
    fn the_picker_takes_the_ticks_once() {
        let mut b = ScheduledBox::Rescheduling {
            selected: vec![MessageId(4), MessageId(5)],
        };
        assert_eq!(b.take_selected(), vec![MessageId(4), MessageId(5)]);
        assert_eq!(b, ScheduledBox::Closed);
        assert!(b.take_selected().is_empty());
    }
}
