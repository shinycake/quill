//! The passcode UI's small stages: the open Settings box, the
//! system-prompt unlock switch, the running job and the auto-lock watcher,
//! each one value with its data inside.
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

/// A slow passcode job (PBKDF2, the system prompt). It belongs to the job,
/// not to the box: closing the box leaves it running, so a lock asked
/// meanwhile still waits for the answer (a Turn off cannot land under it).
/// The stage holds the task that waits for the answer, so the answer is
/// always the held job's; replacing the stage drops the wait.
#[derive(Default)]
pub(crate) enum PasscodeJob {
    #[default]
    Idle,
    Running {
        kind: JobKind,
        fate: JobFate,
        task: Task<()>,
    },
}

/// What a running job is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JobKind {
    /// PBKDF2 on a background thread: it must answer, its result sets or
    /// clears the master key.
    Hashing,
    /// The OS prompt (Touch ID, system password): the user can leave it by
    /// typing the passcode instead.
    SystemPrompt,
}

/// What happens with a running job's answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JobFate {
    /// The answer lands.
    Counts,
    /// A lock was asked while it ran: it lands with the answer.
    ThenLock,
    /// The lock screen's Log out ran meanwhile: the job's thread still
    /// runs, and its answer is dropped (`run_passcode_job`,
    /// `spawn_system_unlock_wait`).
    Cancelled,
}

impl PasscodeJob {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// A passcode check is out: it cannot be left, and the unlock waits.
    pub(crate) fn is_hashing(&self) -> bool {
        matches!(
            self,
            Self::Running {
                kind: JobKind::Hashing,
                ..
            }
        )
    }

    /// The job ends with its answer. Returns what the answer does; an idle
    /// stage answers like a job that counts.
    pub(crate) fn finish(&mut self) -> JobFate {
        match std::mem::take(self) {
            Self::Idle => JobFate::Counts,
            Self::Running { fate, task, .. } => {
                // The task is the caller: let it finish the update it is in.
                task.detach();
                fate
            }
        }
    }

    /// Leave a system prompt that is still out (the user types the
    /// passcode instead): its task and channel are dropped, so a late
    /// answer lands nowhere. A passcode check cannot be left.
    pub(crate) fn leave_system_prompt(&mut self) -> bool {
        let leaves = matches!(
            self,
            Self::Running {
                kind: JobKind::SystemPrompt,
                fate: JobFate::Counts,
                ..
            }
        );
        if leaves {
            *self = Self::Idle;
        }
        leaves
    }

    /// A lock asked while the job runs lands with its answer.
    pub(crate) fn hold_lock(&mut self) {
        if let Self::Running { fate, .. } = self
            && *fate == JobFate::Counts
        {
            *fate = JobFate::ThenLock;
        }
    }

    /// Log out from the lock screen: the answer is dropped.
    pub(crate) fn cancel(&mut self) {
        if let Self::Running { fate, .. } = self {
            *fate = JobFate::Cancelled;
        }
    }
}
