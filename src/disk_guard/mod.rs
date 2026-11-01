//! TDLib cannot survive a failed write to its own storage: a binlog flush
//! that hits `ENOSPC` is `LOG(FATAL)` (`Binlog::flush`, `r_written.ensure()`)
//! and the process aborts, and the same `.ensure()` guards its SQLite calls.
//! The log callback cannot stop that. Opening on a full disk is worse: when
//! `TdDb::init_sqlite` fails, TDLib deletes the SQLite database and starts
//! over.
#![deny(clippy::indexing_slicing)]

mod folders;
pub use folders::database_size;
#[cfg(target_os = "linux")]
mod linux;
mod probe;

pub use probe::{Volume, available_bytes, read_volume};

/// One read of the data folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataReading {
    /// The least free space over the volumes the data folders live on.
    pub free: Option<u64>,
    /// A data folder cannot be written: its read failed, its volume is
    /// read-only, or the volume the data root lived on is gone.
    pub unwritable: bool,
}

/// Reads the data folders under `root` (`folders::data_folders`).
/// `device` is where the root was last seen ([`data_root_device`]).
pub fn read_data(root: &Path, device: Option<u64>) -> DataReading {
    let mut reading = folders::tightest(&folders::data_folders(root), read_volume);
    reading.unwritable |= folders::volume_gone(root, device);
    reading
}

pub fn data_root_device(root: &Path) -> Option<u64> {
    folders::root_device(root)
}

/// Free bytes for TDLib under the data root `root`, with a folder that
/// cannot be written read as no room at all.
pub fn data_free_bytes(root: &Path) -> Option<u64> {
    let reading = read_data(root, None);
    if reading.unwritable {
        Some(0)
    } else {
        reading.free
    }
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
const WRITTEN_WHILE_CONFIRMING: u64 = FASTEST_WRITER_PER_SEC * CONFIRM_READ.as_secs();
const WARN_LEAST: u64 = 1024 * MIB;
const WARN_ABOVE_OPEN: u64 = 256 * MIB;
const WARN_CLEAR_ABOVE_WARN: u64 = 256 * MIB;
const CLOSE_FIXED: u64 = WRITTEN_IN_SHORTEST_READ + WRITTEN_WHILE_CONFIRMING + APFS_RESERVE;

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
        let open = close
            .saturating_add(CATCH_UP_ROOM)
            .saturating_add(data.largest_database);
        // The bar shows before the next open could be refused, however large
        // the database or however many accounts.
        let above_open = open.saturating_add(WARN_ABOVE_OPEN);
        let warn = if above_open > WARN_LEAST {
            above_open
        } else {
            WARN_LEAST
        };
        Self {
            warn,
            warn_clear: warn.saturating_add(WARN_CLEAR_ABOVE_WARN),
            close,
            open,
        }
    }
}

/// The floors for one account: what a reading that knows nothing about the
/// data folders, and the crash breadcrumb, go by.
pub const FLOORS: DiskFloors = DiskFloors::for_data(DataUse::ONE);

const FASTEST_WRITER_PER_SEC: u64 = 64 * MIB;
const SHORTEST_READ: Duration = Duration::from_secs(1);
/// The read step while the disk falls faster than the floors assume.
const QUICKEST_READ: Duration = Duration::from_millis(250);
const LONGEST_READ: Duration = Duration::from_secs(30);
/// A drop below close is read again this much later before clients close:
/// one bad reading (a copy that is deleted at once, a volume that answers
/// late) must not take every account offline.
pub const CONFIRM_READ: Duration = Duration::from_secs(2);

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

impl DiskState {
    /// [`Self::next`], where a first reading below close only arms
    /// `below_once`, and Full needs the next reading to agree. Returns the
    /// state and the new `below_once`.
    pub fn confirmed(
        self,
        free: Option<u64>,
        floors: &DiskFloors,
        below_once: bool,
    ) -> (Self, bool) {
        if free.is_none() {
            return (self, below_once);
        }
        let after = self.next(free, floors);
        if after != Self::Full || self == Self::Full || below_once {
            return (after, false);
        }
        (self, true)
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

/// How long until the next read: the time a writer needs to bring `free`
/// down to close, at the fastest writer the floors assume or at the fall
/// the last two readings showed, whichever is faster.
pub fn next_read_in(free: Option<u64>, floors: &DiskFloors, fall_per_sec: u64) -> Duration {
    let Some(free) = free else {
        return LONGEST_READ;
    };
    let shortest = if fall_per_sec > FASTEST_WRITER_PER_SEC {
        QUICKEST_READ
    } else {
        SHORTEST_READ
    };
    let rate = fall_per_sec.max(FASTEST_WRITER_PER_SEC);
    let millis = free.saturating_sub(floors.close).saturating_mul(1000) / rate;
    Duration::from_millis(millis).clamp(shortest, LONGEST_READ)
}

/// Bytes per second the disk lost between two readings `waited` apart.
pub fn fall_per_sec(before: Option<u64>, after: Option<u64>, waited: Duration) -> u64 {
    let (Some(before), Some(after)) = (before, after) else {
        return 0;
    };
    let millis = u64::try_from(waited.as_millis()).unwrap_or(u64::MAX).max(1);
    before.saturating_sub(after).saturating_mul(1000) / millis
}

#[cfg(test)]
mod tests;
