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

/// The floors for what is under the data root `root` now.
pub fn data_floors(root: &Path) -> DiskFloors {
    DiskFloors::for_data(folders::data_use(root))
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

/// What one account's close can write: TDLib's close flushes the binlog
/// and checkpoints the SQLite WAL (about 4 MiB at SQLite's default 1000
/// pages), and until the binlog closes any new event can start a reindex,
/// which writes a copy of its live events (logged as large past 10 MiB).
const CLOSE_WRITES_PER_ACCOUNT: u64 = 14 * MIB;
/// Free space `statfs` reports and APFS will not hand out: a new file failed
/// with 32.1 MiB reported free on 2, 4 and 8 GiB volumes.
const APFS_RESERVE: u64 = 32 * MIB;
const CATCH_UP_ROOM: u64 = 128 * MIB;
const WRITTEN_IN_SHORTEST_READ: u64 = FASTEST_WRITER_PER_SEC * SHORTEST_READ.as_secs();
const CLOSE_FIXED: u64 = WRITTEN_IN_SHORTEST_READ + APFS_RESERVE;

/// What the data folders hold that the floors scale with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataUse {
    /// Accounts with a database: each one's client closes at Full.
    pub accounts: u64,
    /// The largest account's `db.sqlite` with its WAL. A TDLib update can
    /// migrate the database while it opens, and SQLite may rewrite every
    /// page of it into the WAL before the checkpoint.
    pub largest_database: u64,
}

impl DataUse {
    /// One account with an empty database: the least any reading can mean.
    pub const ONE: Self = Self {
        accounts: 1,
        largest_database: 0,
    };
}

impl DiskFloors {
    pub const fn for_data(data: DataUse) -> Self {
        let accounts = if data.accounts == 0 { 1 } else { data.accounts };
        let close = CLOSE_FIXED.saturating_add(CLOSE_WRITES_PER_ACCOUNT.saturating_mul(accounts));
        Self {
            warn: 1024 * MIB,
            warn_clear: 1280 * MIB,
            close,
            open: close
                .saturating_add(CATCH_UP_ROOM)
                .saturating_add(data.largest_database),
        }
    }
}

/// The floors for one account: what a reading that knows nothing about the
/// data folders, and the crash breadcrumb, go by.
pub const FLOORS: DiskFloors = DiskFloors::for_data(DataUse::ONE);

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
