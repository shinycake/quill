//! Keeps TDLib away from a full disk.
//!
//! TDLib cannot survive a failed write to its own storage: a binlog flush
//! that hits `ENOSPC` is `LOG(FATAL)` (`Binlog::flush`, `r_written.ensure()`)
//! and the process aborts, and the same `.ensure()` guards its SQLite calls.
//! The log callback cannot stop that. Opening on a full disk is worse: when
//! `TdDb::init_sqlite` fails, TDLib deletes the SQLite database and starts
//! over. So Quill stops before TDLib gets there: a read of the free bytes on
//! the data volume moves one [`DiskState`], and everything the user sees and
//! every client action follows from that state.
//!
//! Neither tdesktop nor Telegram X has a floor to copy. tdesktop has no
//! TDLib, and a failed write only cancels that download
//! (`FailureReason::FileWriteFailure`). Telegram X lets TDLib abort and
//! names the disk on the next launch (`Crash.Type.DISK_FULL`). The numbers
//! in [`FLOORS`] come from what TDLib writes, as each field says.
#![deny(clippy::indexing_slicing)]

mod probe;

pub use probe::available_bytes;

use std::time::Duration;

const MIB: u64 = 1 << 20;

/// The free bytes where behavior changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskFloors {
    /// Below this the bar shows and automatic downloads stop. About 20 of
    /// the largest automatic download (`AUTO_DOWNLOAD_MAX_BYTES`, 50 MiB),
    /// so the warning comes while there is still room to act on it.
    pub warn: u64,
    /// The bar leaves at this much free, above `warn`, so a disk hovering
    /// at the floor does not flicker it.
    pub warn_clear: u64,
    /// Below this every client closes. A clean close still writes: one
    /// binlog rewrite (a copy of its live events, which TDLib calls large
    /// past 10 MiB) and one SQLite WAL checkpoint (4 MiB at the default
    /// 1000 pages), for each open account. That is 42 MiB for three
    /// accounts, rounded up to 64 MiB, plus one [`next_read_in`] step at
    /// [`FASTEST_WRITER`].
    pub close: u64,
    /// A client opens only with this much free: `close` plus room for the
    /// catch-up sync, so a client that opens is not closed again at once.
    pub open: u64,
}

/// The one place to change a floor.
pub const FLOORS: DiskFloors = DiskFloors {
    warn: 1024 * MIB,
    warn_clear: 1280 * MIB,
    close: 128 * MIB,
    open: 256 * MIB,
};

/// The fastest write the reads have to keep up with: a download at about
/// 500 Mbit/s. Anything faster (a large local copy) can pass `close`
/// between two reads, and TDLib's abort is what remains for that case.
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
    /// The state after a read. `None` (a volume that reports no sizes, or
    /// a failed read) says nothing about the disk: the state stays.
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

/// Whether a client must not open with `free` bytes on the volume. A read
/// that says nothing never refuses: the disk may be fine.
pub fn refuses_open(free: Option<u64>, floors: &DiskFloors) -> bool {
    free.is_some_and(|free| free < floors.open)
}

/// Whether TDLib's last words came with the disk full.
pub fn was_full(free: Option<u64>, floors: &DiskFloors) -> bool {
    free.is_some_and(|free| free < floors.close)
}

/// When to read again: the time [`FASTEST_WRITER`] needs to reach `close`,
/// so a healthy disk costs one read in [`LONGEST_READ`] and only a nearly
/// full one is read every [`SHORTEST_READ`].
pub fn next_read_in(free: Option<u64>, floors: &DiskFloors) -> Duration {
    let Some(free) = free else {
        return LONGEST_READ;
    };
    Duration::from_secs(free.saturating_sub(floors.close) / FASTEST_WRITER)
        .clamp(SHORTEST_READ, LONGEST_READ)
}

#[cfg(test)]
mod tests;
