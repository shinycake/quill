//! The scheduled-messages dialog and the schedule picker as one flow on the
//! chat view (`ChatUi::schedule`).
#![deny(clippy::indexing_slicing)]

use super::composer::ScheduledDialog;
use super::scheduled::SchedulePicker;
use quill::ids::RequestId;

/// The scheduled-messages dialog and the schedule picker are two boxes of
/// one flow (Reschedule leaves the list for the picker, and Cancel comes
/// back), never open together: one value on the chat.
#[derive(Default)]
pub(super) enum ScheduleFlow {
    #[default]
    Closed,
    /// The scheduled-messages dialog, with the stage of the load that fills
    /// it (never `ScheduledDialog::Closed`: that is this flow's `Closed`).
    List(ScheduledDialog),
    /// The picker popup with the date+time picker it holds (created when
    /// the popup opens, see `open_schedule_picker`).
    Picking(SchedulePicker),
}

impl ScheduleFlow {
    pub(super) fn picker(&self) -> Option<&SchedulePicker> {
        match self {
            Self::Picking(picker) => Some(picker),
            Self::Closed | Self::List(_) => None,
        }
    }

    pub(super) fn picker_mut(&mut self) -> Option<&mut SchedulePicker> {
        match self {
            Self::Picking(picker) => Some(picker),
            Self::Closed | Self::List(_) => None,
        }
    }

    /// The picker goes (the flow is closed); the list is not the picker's
    /// to close.
    pub(super) fn take_picker(&mut self) -> Option<SchedulePicker> {
        match std::mem::take(self) {
            Self::Picking(picker) => Some(picker),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn list_open(&self) -> bool {
        matches!(self, Self::List(_))
    }

    /// The dialog's load stage; `Closed` while the list is not the open box.
    pub(super) fn dialog(&self) -> &ScheduledDialog {
        match self {
            Self::List(stage) => stage,
            Self::Closed | Self::Picking(_) => &ScheduledDialog::Closed,
        }
    }

    /// The list opens at `stage` (a picker or an earlier list goes).
    pub(super) fn open_list(&mut self, stage: ScheduledDialog) {
        *self = if stage.is_open() {
            Self::List(stage)
        } else {
            Self::Closed
        };
    }

    /// The load moves to `stage`; only an open list has a load.
    pub(super) fn set_list_stage(&mut self, stage: ScheduledDialog) {
        if let Self::List(current) = self {
            *current = stage;
        }
    }

    /// The list closes, with the load it still waits for (its request); a
    /// picker that replaced it stays.
    pub(super) fn close_list(&mut self) -> Option<RequestId> {
        if !self.list_open() {
            return None;
        }
        let Self::List(stage) = std::mem::take(self) else {
            return None;
        };
        stage.loading()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_closes_with_the_load_it_still_waits_for() {
        let request = RequestId(7);
        let mut flow = ScheduleFlow::default();
        flow.open_list(ScheduledDialog::Loading(request));
        assert!(flow.list_open());
        assert_eq!(flow.dialog().loading(), Some(request));
        flow.set_list_stage(ScheduledDialog::Ready);
        assert_eq!(flow.dialog(), &ScheduledDialog::Ready);
        flow.set_list_stage(ScheduledDialog::Loading(request));
        assert_eq!(flow.close_list(), Some(request));
        assert!(!flow.list_open());
        assert_eq!(flow.close_list(), None, "closed twice leaves nothing");
    }

    #[test]
    fn a_closed_flow_has_no_load_to_settle() {
        let mut flow = ScheduleFlow::default();
        flow.set_list_stage(ScheduledDialog::Ready);
        assert!(!flow.list_open(), "only an open list has a load");
        flow.open_list(ScheduledDialog::Closed);
        assert!(!flow.list_open());
        assert_eq!(flow.dialog(), &ScheduledDialog::Closed);
    }
}
