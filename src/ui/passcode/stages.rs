//! The passcode UI's two small stages: the open Settings box and the
//! system-prompt unlock switch, each one value with its data inside.
#![deny(clippy::indexing_slicing)]

use super::PasscodeView;
use gpui_kit::Task;
use quill::passcode::MasterKey;

/// Touch ID / system-prompt unlock: off, or on with the master key kept
/// while locked so the prompt can unlock again without the passcode
/// (tdesktop: "enter your passcode before you can use Touch ID"). The key
/// exists only inside `On`, so it cannot outlive the switch.
pub(crate) enum SystemUnlock {
    Off,
    On { held_key: Option<MasterKey> },
}

impl SystemUnlock {
    pub(crate) fn is_on(&self) -> bool {
        matches!(self, Self::On { .. })
    }

    pub(super) fn held_key(&self) -> Option<&MasterKey> {
        match self {
            Self::On { held_key } => held_key.as_ref(),
            Self::Off => None,
        }
    }

    /// Keep `key` for the next unlock; nothing is kept while the switch is off.
    pub(super) fn hold(&mut self, key: Option<MasterKey>) {
        if let Self::On { held_key } = self {
            *held_key = key;
        }
    }
}

/// The open Settings passcode box: its page and the line it shows.
/// Closing drops it whole, so no page or error outlives the box.
#[derive(Clone, Debug, Default)]
pub(crate) struct PasscodeBox {
    pub(crate) view: PasscodeView,
    pub(crate) error: Option<String>,
}

/// Locked or not, with the number of locks so far (a locked start counts).
/// Something parked for later records the count, and a lock in between
/// drops it (`PasscodeUi::locked_since`), whether or not Quill is still
/// locked when it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LockState {
    Unlocked { locks: u64 },
    Locked { locks: u64 },
}

impl LockState {
    pub(super) fn new(locked: bool) -> Self {
        Self::Unlocked { locks: 0 }.set(locked)
    }

    pub(super) fn locks(self) -> u64 {
        match self {
            Self::Unlocked { locks } | Self::Locked { locks } => locks,
        }
    }

    /// A lock counts once, when it starts.
    pub(super) fn set(self, locked: bool) -> Self {
        match (self, locked) {
            (Self::Unlocked { locks }, true) => Self::Locked {
                locks: locks.saturating_add(1),
            },
            (Self::Locked { locks }, false) => Self::Unlocked { locks },
            (same, _) => same,
        }
    }
}

/// The auto-lock watcher (`spawn_passcode_tick`): not started, or running
/// as the task this value holds, so the app going away drops it.
#[derive(Default)]
pub(crate) enum AutoLock {
    #[default]
    Stopped,
    Watching {
        _task: Task<()>,
    },
}
