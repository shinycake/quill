//! TDLib cannot survive a failed write to its own storage: a binlog flush
//! that hits `ENOSPC` is `LOG(FATAL)` (`Binlog::flush`, `r_written.ensure()`)
//! and the process aborts, and the same `.ensure()` guards its SQLite calls.
//! The log callback cannot stop that. Opening on a full disk is worse: when
//! `TdDb::init_sqlite` fails, TDLib deletes the SQLite database and starts
//! over.
#![deny(clippy::indexing_slicing)]

mod probe;

pub use probe::available_bytes;

use std::time::Duration;

const MIB: u64 = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskFloors {
    /// About 20 of the largest automatic download (`AUTO_DOWNLOAD_MAX_BYTES`,
    /// 50 MiB), so the warning comes while there is still room to act on it.
    pub warn: u64,
    pub warn_clear: u64,
    /// A clean close still writes: one binlog rewrite (a copy of its live
    /// events, which TDLib calls large past 10 MiB) and one SQLite WAL
    /// checkpoint (4 MiB at the default 1000 pages), for each open account.
    /// That is 42 MiB for three accounts, rounded up to [`CLOSE_WRITES`],
    /// plus one [`next_read_in`] step at [`FASTEST_WRITER`], plus
    /// [`APFS_RESERVE`].
    pub close: u64,
    pub open: u64,
}

const CLOSE_WRITES: u64 = 64 * MIB;
/// Free space `statfs` reports and APFS will not hand out: a new file failed
/// with 32.1 MiB reported free on 2, 4 and 8 GiB volumes.
const APFS_RESERVE: u64 = 32 * MIB;
const CATCH_UP_ROOM: u64 = 128 * MIB;
const CLOSE_FLOOR: u64 = CLOSE_WRITES + FASTEST_WRITER + APFS_RESERVE;

pub const FLOORS: DiskFloors = DiskFloors {
    warn: 1024 * MIB,
    warn_clear: 1280 * MIB,
    close: CLOSE_FLOOR,
    open: CLOSE_FLOOR + CATCH_UP_ROOM,
};

/// The fastest write the reads have to keep up with: a download at about
/// 500 Mbit/s.
const FASTEST_WRITER: u64 = 64 * MIB;
const SHORTEST_READ: Duration = Duration::from_secs(1);
const LONGEST_READ: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiskState {
    #[default]
    Healthy,
    /// Clients run; the bar shows and automatic downloads wait.
    Low,
    /// No client runs. Holds until `open` is free, however it was reached.
    Full,
}

impl DiskState {
    /// `None` is no reading: the state stays.
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
    /// Close every client and end the starts in flight.
    Close,
    Reconnect,
    RetryStarts,
}

impl DiskState {
    /// `running`: a client is up. `retry`: a refused start can go again.
    pub fn step(self, after: Self, running: bool, retry: bool) -> DiskStep {
        match (self, after) {
            (Self::Full, Self::Full) if !running => DiskStep::Hold,
            (_, Self::Full) => DiskStep::Close,
            (Self::Full, _) => DiskStep::Reconnect,
            _ if retry => DiskStep::RetryStarts,
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
    Duration::from_secs(free.saturating_sub(floors.close) / FASTEST_WRITER)
        .clamp(SHORTEST_READ, LONGEST_READ)
}

#[cfg(test)]
mod tests;
