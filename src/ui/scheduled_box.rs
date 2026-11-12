//! The scheduled-messages box of a chat: whether it is on screen, the load
//! of its list and the messages ticked in it. One value, so the ticks and
//! the load cannot outlive the box or be read while it is shut. It lives in
//! the chat's `ChatUi`, so a chat change drops it with the chat.

use quill::ids::{MessageId, RequestId};

/// The list of an open box: `Loading` leaves on its request's answer
/// (`Ready`, or `Failed` with the line the box draws beside a Retry), or
/// with the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScheduledLoad {
    Loading(RequestId),
    Ready,
    Failed(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) enum ScheduledBox {
    #[default]
    Closed,
    Open {
        load: ScheduledLoad,
        selected: Vec<MessageId>,
    },
    /// The box is hidden while the schedule picker asks for a new time
    /// (Reschedule); the ticked messages wait for it or for Cancel.
    Rescheduling { selected: Vec<MessageId> },
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
            Self::Open { selected, .. } | Self::Rescheduling { selected } => selected,
        }
    }

    /// The list's load, while the box is on screen.
    pub(super) fn load(&self) -> Option<&ScheduledLoad> {
        match self {
            Self::Open { load, .. } => Some(load),
            Self::Closed | Self::Rescheduling { .. } => None,
        }
    }

    /// The request the open box waits for.
    pub(super) fn loading(&self) -> Option<RequestId> {
        match self.load() {
            Some(ScheduledLoad::Loading(request)) => Some(*request),
            _ => None,
        }
    }

    /// The box opens with nothing ticked.
    pub(super) fn open(&mut self, load: ScheduledLoad) {
        *self = Self::Open {
            load,
            selected: Vec::new(),
        };
    }

    /// The box opens with its list at hand and nothing ticked.
    #[cfg(test)]
    pub(super) fn open_fresh(&mut self) {
        self.open(ScheduledLoad::Ready);
    }

    /// The box comes back after the picker, or loads its list again: ticks
    /// that waited for it stay.
    pub(super) fn resume(&mut self, load: ScheduledLoad) {
        let selected = match std::mem::take(self) {
            Self::Rescheduling { selected } | Self::Open { selected, .. } => selected,
            Self::Closed => Vec::new(),
        };
        *self = Self::Open { load, selected };
    }

    /// `request` was answered: the load it started ends. Any other answer,
    /// and one for a box that closed since, changes nothing.
    pub(super) fn settle(&mut self, request: RequestId, answer: ScheduledLoad) {
        if let Self::Open { load, .. } = self
            && *load == ScheduledLoad::Loading(request)
        {
            *load = answer;
        }
    }

    /// The box hides for the schedule picker; the ticks wait.
    pub(super) fn hide_for_picker(&mut self) {
        *self = match std::mem::take(self) {
            Self::Open { selected, .. } | Self::Rescheduling { selected } => {
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
            Self::Open { selected, .. } | Self::Rescheduling { selected } => selected,
            Self::Closed => Vec::new(),
        }
    }

    pub(super) fn clear_selection(&mut self) {
        if let Self::Open { selected, .. } | Self::Rescheduling { selected } = self {
            selected.clear();
        }
    }

    /// A tick of `id` flipped (only while the box is open).
    pub(super) fn toggle(&mut self, id: MessageId) {
        if let Self::Open { selected, .. } = self {
            quill::selection_pin::toggle_id(selected, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ScheduledBox, ScheduledLoad};
    use quill::ids::{MessageId, RequestId};

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
        b.resume(ScheduledLoad::Ready);
        assert!(b.is_open());
        assert_eq!(
            b.selected(),
            &[MessageId(2)],
            "Cancel brings the ticks back"
        );
        b.close();
        assert!(b.selected().is_empty(), "closing drops the ticks");
        b.resume(ScheduledLoad::Ready);
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

    #[test]
    fn only_the_answer_of_the_load_under_way_ends_it() {
        let mut b = ScheduledBox::default();
        b.open(ScheduledLoad::Loading(RequestId(7)));
        assert_eq!(b.loading(), Some(RequestId(7)));
        b.settle(RequestId(6), ScheduledLoad::Failed("old".into()));
        assert_eq!(b.loading(), Some(RequestId(7)), "an older answer is not it");
        b.settle(RequestId(7), ScheduledLoad::Ready);
        assert_eq!(b.load(), Some(&ScheduledLoad::Ready));
        b.settle(RequestId(7), ScheduledLoad::Failed("again".into()));
        assert_eq!(b.load(), Some(&ScheduledLoad::Ready), "answered once");
    }

    #[test]
    fn a_hidden_or_closed_box_has_no_load() {
        let mut b = ScheduledBox::default();
        b.open(ScheduledLoad::Loading(RequestId(3)));
        b.hide_for_picker();
        assert_eq!(b.loading(), None);
        b.settle(RequestId(3), ScheduledLoad::Ready);
        assert!(!b.is_open(), "an answer does not bring the box back");
        b.close();
        b.settle(RequestId(3), ScheduledLoad::Ready);
        assert_eq!(b, ScheduledBox::Closed);
    }
}
