//! The trusted prices file of the account (split out of [`super`]): read
//! once, written when the prices change.

use super::*;

/// The trusted prices file of the account: read once, written when the
/// prices change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TrustFile {
    /// Not read yet.
    #[default]
    NotLoaded,
    /// The file exists but could not be read (permissions, I/O): nothing is
    /// written over it until a read succeeds, which is tried again at
    /// `retry_at_ms` (unix milliseconds). Prices agreed to meanwhile are kept
    /// in memory and join the file's when it is read.
    LoadFailed { retry_at_ms: u64 },
    /// Read, and the prices match the file.
    Clean,
    /// The prices changed and are not written yet.
    Dirty,
    /// The last write failed `tries` times in a row (the reason, for the
    /// person to see); the next change writes again, and so does the first
    /// tick after `retry_at_ms` (unix milliseconds).
    SaveFailed {
        reason: String,
        retry_at_ms: u64,
        tries: u8,
    },
}

/// How long a failed write of the trusted prices waits before the next try;
/// each further failure doubles the wait, up to [`TRUST_RETRY_MAX_MS`].
pub const TRUST_RETRY_MS: u64 = 30_000;

/// The longest wait between two tries of a failing write.
pub const TRUST_RETRY_MAX_MS: u64 = 15 * 60_000;

/// The wait after `tries` failed writes in a row.
fn trust_retry_after(tries: u8) -> u64 {
    TRUST_RETRY_MS
        .saturating_mul(1 << tries.saturating_sub(1).min(10))
        .min(TRUST_RETRY_MAX_MS)
}

impl PaidMessages {
    /// The user agreed to `stars` per message in `chat_id`: remembered, and
    /// the file is behind.
    pub fn remember_trust(&mut self, chat_id: i64, stars: i64) {
        self.trust.trust(chat_id, stars);
        self.trust_changed();
    }

    /// The prices changed: the file is behind, unless it could not be read
    /// (it is then not overwritten, and the next read merges these in).
    pub(super) fn trust_changed(&mut self) {
        if !matches!(self.trust_file, TrustFile::LoadFailed { .. }) {
            self.trust_file = TrustFile::Dirty;
        }
    }

    /// Whether a send of `per_message` Stars to `chat_id` needs the
    /// confirmation box.
    pub fn needs_confirmation(&self, chat_id: i64, per_message: i64) -> bool {
        needs_confirmation(&self.trust, chat_id, per_message)
    }

    /// The trusted prices, per chat, for the send guard.
    pub fn trusted_prices(&self) -> HashMap<i64, i64> {
        self.trust.entries()
    }

    /// The file's text when the prices changed since it was written, or the
    /// last write failed and its wait is over at `now_ms`.
    pub fn trust_to_write(&self, now_ms: u64) -> Option<String> {
        let due = match &self.trust_file {
            TrustFile::Dirty => true,
            TrustFile::SaveFailed { retry_at_ms, .. } => now_ms >= *retry_at_ms,
            TrustFile::NotLoaded | TrustFile::LoadFailed { .. } | TrustFile::Clean => false,
        };
        due.then(|| self.trust.to_json())
    }

    /// Whether the file is to be read now: not read yet, or its last read
    /// failed and the wait is over at `now_ms`.
    pub fn trust_read_due(&self, now_ms: u64) -> bool {
        match &self.trust_file {
            TrustFile::NotLoaded => true,
            TrustFile::LoadFailed { retry_at_ms } => now_ms >= *retry_at_ms,
            _ => false,
        }
    }

    /// The file was read (`None`: there is none): its prices are the
    /// trusted ones, once. Prices agreed to while a read had failed join
    /// them, and the file is then behind.
    pub fn trust_loaded(&mut self, text: Option<&str>) {
        let kept = match self.trust_file {
            TrustFile::NotLoaded => PaidTrust::default(),
            TrustFile::LoadFailed { .. } => std::mem::take(&mut self.trust),
            _ => return,
        };
        self.trust_file = TrustFile::Clean;
        if let Some(text) = text {
            self.trust = PaidTrust::from_json(text);
        }
        let joined = kept.entries();
        for (chat_id, stars) in &joined {
            self.trust.trust(*chat_id, *stars);
        }
        if !joined.is_empty() {
            self.trust_file = TrustFile::Dirty;
        }
    }

    /// The file exists but could not be read at `now_ms`: it is kept as it
    /// is, and read again after a wait.
    pub fn trust_load_failed(&mut self, now_ms: u64) {
        if self.trust_read_due(now_ms) {
            self.trust_file = TrustFile::LoadFailed {
                retry_at_ms: now_ms.saturating_add(TRUST_RETRY_MS),
            };
        }
    }

    /// The write of [`Self::trust_to_write`] ended: the file is clean, or
    /// the reason it failed.
    pub fn trust_written(&mut self, result: Result<(), String>, now_ms: u64) {
        let failed_before = match &self.trust_file {
            TrustFile::SaveFailed { tries, .. } => *tries,
            _ => 0,
        };
        self.trust_file = match result {
            Ok(()) => TrustFile::Clean,
            Err(reason) => {
                let tries = failed_before.saturating_add(1);
                TrustFile::SaveFailed {
                    reason,
                    retry_at_ms: now_ms.saturating_add(trust_retry_after(tries)),
                    tries,
                }
            }
        };
    }

    /// The reason the last write of the trusted prices failed.
    pub fn trust_save_error(&self) -> Option<&str> {
        match &self.trust_file {
            TrustFile::SaveFailed { reason, .. } => Some(reason),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sweep r4-04 fix 3: a file that cannot be read is not "no file": nothing
    /// is written over it, and the prices agreed to meanwhile join its prices
    /// when a later read works.
    #[test]
    fn an_unreadable_trust_file_is_not_overwritten() {
        let mut state = PaidMessages::default();
        assert!(state.trust_read_due(0));
        state.trust_load_failed(1_000);
        assert!(!state.trust_read_due(1_000));
        assert!(state.trust_read_due(1_000 + TRUST_RETRY_MS));
        state.remember_trust(5, 7);
        assert!(!state.needs_confirmation(5, 7), "kept in memory");
        assert!(state.trust_to_write(u64::MAX).is_none(), "no write over it");
        state.trust_loaded(Some(r#"{"4": 10}"#));
        assert!(!state.needs_confirmation(4, 10), "the file's price");
        assert!(!state.needs_confirmation(5, 7), "the new price");
        assert!(state.trust_to_write(0).is_some(), "the file is behind");
        // A missing file is no prices, and reads once.
        let mut fresh = PaidMessages::default();
        fresh.trust_loaded(None);
        assert!(!fresh.trust_read_due(u64::MAX));
        assert!(fresh.trust_to_write(u64::MAX).is_none());
    }
}
