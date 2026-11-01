//! TDLib cannot survive a failed write to its own storage: a binlog flush
//! that hits `ENOSPC` is `LOG(FATAL)` (`Binlog::flush`, `r_written.ensure()`)
//! and the process aborts, and the same `.ensure()` guards its SQLite calls.
//! The log callback cannot stop that. Opening on a full disk is worse: when
//! `TdDb::init_sqlite` fails, TDLib deletes the SQLite database and starts
//! over.
#![deny(clippy::indexing_slicing)]

mod folders;
#[cfg(target_os = "linux")]
mod linux;
mod probe;

pub use probe::available_bytes;

/// Free bytes for TDLib under the data root `root`: the least over the
/// volumes its data folders live on (`folders::data_folders`).
pub fn data_free_bytes(root: &Path) -> Option<u64> {
    folders::tightest(&folders::data_folders(root), available_bytes)
}

use std::path::Path;
use std::time::Duration;

const MIB: u64 = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskFloors {
    pub warn: u64,
    pub warn_clear: u64,
    pub close: u64,
    pub open: u64,
}

/// TDLib's close flushes each binlog and checkpoints each SQLite WAL, and
/// until a binlog closes any new event can start a reindex, which writes a
/// copy of its live events.
const CLOSE_WRITES: u64 = 64 * MIB;
/// Free space `statfs` reports and APFS will not hand out: a new file failed
/// with 32.1 MiB reported free on 2, 4 and 8 GiB volumes.
const APFS_RESERVE: u64 = 32 * MIB;
const CATCH_UP_ROOM: u64 = 128 * MIB;
const WRITTEN_IN_SHORTEST_READ: u64 = FASTEST_WRITER_PER_SEC * SHORTEST_READ.as_secs();
const CLOSE_FLOOR: u64 = CLOSE_WRITES + WRITTEN_IN_SHORTEST_READ + APFS_RESERVE;

pub const FLOORS: DiskFloors = DiskFloors {
    warn: 1024 * MIB,
    warn_clear: 1280 * MIB,
    close: CLOSE_FLOOR,
    open: CLOSE_FLOOR + CATCH_UP_ROOM,
};

const FASTEST_WRITER_PER_SEC: u64 = 64 * MIB;
const SHORTEST_READ: Duration = Duration::from_secs(1);
const LONGEST_READ: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiskState {
    #[default]
    Healthy,
    Low,
    Full,
}

impl DiskState {
    pub fn next(self, free: Option<u64>, floors: &DiskFloors) -> Self {
        let Some(free) = free else {
            return self;
        };
        match self {
            Self::Full if free < floors.open => Self::Full,
            _ if free < floors.close => Self::Full,
            Self::Healthy if free >= floors.warn => Self::Healthy,
            Self::Low | Self::Full if free >= floors.warn_clear => Self::Healthy,
            _ => Self::Low,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskStep {
    Hold,
    Close,
    Reconnect,
    RetryStarts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskClients {
    pub running: bool,
    pub retry_starts: bool,
}

impl DiskState {
    pub fn step(self, after: Self, clients: DiskClients) -> DiskStep {
        match (self, after) {
            (Self::Full, Self::Full) if !clients.running => DiskStep::Hold,
            (_, Self::Full) => DiskStep::Close,
            (Self::Full, _) => DiskStep::Reconnect,
            _ if clients.retry_starts => DiskStep::RetryStarts,
            _ => DiskStep::Hold,
        }
    }
}

pub fn refuses_open(free: Option<u64>, floors: &DiskFloors) -> bool {
    free.is_some_and(|free| free < floors.open)
}

pub fn was_full(free: Option<u64>, floors: &DiskFloors) -> bool {
    free.is_some_and(|free| free < floors.close)
}

pub fn next_read_in(free: Option<u64>, floors: &DiskFloors) -> Duration {
    let Some(free) = free else {
        return LONGEST_READ;
    };
    Duration::from_secs(free.saturating_sub(floors.close) / FASTEST_WRITER_PER_SEC)
        .clamp(SHORTEST_READ, LONGEST_READ)
}

#[cfg(test)]
mod tests;
