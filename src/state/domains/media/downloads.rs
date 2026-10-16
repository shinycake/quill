//! What the user (or an automatic path) asked to download, and the status
//! every row shows. TDLib's file state (`ParsedFile::local`) says whether a
//! transfer is active or finished; this module only stores what TDLib
//! cannot know: who asked, whether the user paused it, and how a request
//! ended. Status is computed from both, never stored.
use std::collections::{HashMap, HashSet, VecDeque};

use crate::telegram::envelope::ParsedFile;

const RECENT_CAP: usize = 50;

/// Who asked for the download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadOrigin {
    /// Thumbnails, avatars, sounds, auto-download of chat media.
    Auto,
    /// A history row, the viewer or the downloads manager. Listed in the
    /// downloads manager and pausable.
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The request is out; TDLib's file state tells how far it is.
    Requested,
    /// The request ended without completing (an error answer, or an active
    /// to idle update). User requests offer Retry; automatic ones are not
    /// re-sent on every ingest.
    Ended,
    /// The user cancelled. Hides a stale `is_downloading_active` until
    /// TDLib confirms with an idle update.
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Intent {
    origin: DownloadOrigin,
    paused: bool,
    phase: Phase,
}

/// What a row shows for a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadStatus {
    Idle,
    Downloading,
    Paused,
    Failed,
    Done,
}

/// One intent per file id plus the log of finished user downloads.
///
/// `entries` also holds every thumbnail and avatar request, so the counts
/// and id lists the UI reads each frame come from small indexes kept in
/// step by `put`, the only writer of `entries`.
#[derive(Clone, Debug, Default)]
pub struct DownloadIntents {
    entries: HashMap<i32, Intent>,
    finished: VecDeque<i32>,
    /// Requests out and not cancelled, any origin.
    requested: usize,
    /// Requested user downloads.
    user: HashSet<i32>,
    /// User downloads that ended without completing.
    failed: HashSet<i32>,
}

impl DownloadIntents {
    /// The request is out (retry clears an earlier end).
    pub fn begin(&mut self, file_id: i32, origin: DownloadOrigin) {
        if file_id == 0 {
            return;
        }
        let paused = self
            .entries
            .get(&file_id)
            .is_some_and(|i| i.phase == Phase::Requested && i.paused && origin == i.origin);
        self.put(
            file_id,
            Some(Intent {
                origin,
                paused,
                phase: Phase::Requested,
            }),
        );
    }

    /// The single writer of `entries`: replaces or removes one intent and
    /// keeps the indexes in step.
    fn put(&mut self, file_id: i32, intent: Option<Intent>) {
        let old = match intent {
            Some(new) => self.entries.insert(file_id, new),
            None => self.entries.remove(&file_id),
        };
        if let Some(old) = old {
            if old.phase == Phase::Requested {
                self.requested = self.requested.saturating_sub(1);
            }
            self.user.remove(&file_id);
            self.failed.remove(&file_id);
        }
        if let Some(new) = intent {
            match (new.phase, new.origin) {
                (Phase::Requested, DownloadOrigin::User) => {
                    self.user.insert(file_id);
                }
                (Phase::Ended, DownloadOrigin::User) => {
                    self.failed.insert(file_id);
                }
                _ => {}
            }
            if new.phase == Phase::Requested {
                self.requested += 1;
            }
        }
    }

    /// The request ended without completing. A user's cancel is not a
    /// failure: TDLib answers the cancelled request with an error.
    pub fn end(&mut self, file_id: i32) {
        match self.get(file_id) {
            Some(i) if i.phase == Phase::Cancelled => self.put(file_id, None),
            Some(i) => {
                let origin = i.origin;
                self.end_as(file_id, origin);
            }
            None => self.end_as(file_id, DownloadOrigin::Auto),
        }
    }

    /// TDLib reports the transfer idle: the cancel took effect.
    pub fn confirm_cancel(&mut self, file_id: i32) {
        if self
            .get(file_id)
            .is_some_and(|i| i.phase == Phase::Cancelled)
        {
            self.put(file_id, None);
        }
    }

    pub fn end_as(&mut self, file_id: i32, origin: DownloadOrigin) {
        self.put(
            file_id,
            Some(Intent {
                origin,
                paused: false,
                phase: Phase::Ended,
            }),
        );
    }

    /// Stop tracking an in-flight request (completed, undownloadable, or
    /// the file left the list). A recorded end stays.
    pub fn settle(&mut self, file_id: i32) {
        if self
            .entries
            .get(&file_id)
            .is_some_and(|i| i.phase != Phase::Ended)
        {
            self.put(file_id, None);
        }
    }

    /// The user cancelled.
    pub fn cancel(&mut self, file_id: i32) {
        if let Some(intent) = self.get(file_id).copied()
            && intent.phase == Phase::Requested
        {
            self.put(
                file_id,
                Some(Intent {
                    phase: Phase::Cancelled,
                    paused: false,
                    ..intent
                }),
            );
        }
    }

    /// An explicit request after a stall: let the automatic path try again.
    pub fn clear_stalled(&mut self, file_id: i32) {
        if self.is_stalled_auto(file_id) {
            self.put(file_id, None);
        }
    }

    /// Forget any record of the file (it left the downloads list, or it
    /// finished and nothing needs remembering).
    pub fn forget(&mut self, file_id: i32) {
        self.put(file_id, None);
    }

    /// Pause state from the downloads list; only a requested user download
    /// can be paused.
    pub fn set_paused(&mut self, file_id: i32, paused: bool) {
        if let Some(intent) = self.get(file_id).copied()
            && intent.origin == DownloadOrigin::User
            && intent.phase == Phase::Requested
        {
            self.put(file_id, Some(Intent { paused, ..intent }));
        }
    }

    /// A user download the list API reports, possibly started elsewhere.
    pub fn adopt(&mut self, file_id: i32, paused: bool) {
        self.begin(file_id, DownloadOrigin::User);
        self.set_paused(file_id, paused);
    }

    /// A finished user download enters the recent list (deduped, capped).
    pub fn record_finished(&mut self, file_id: i32) {
        self.finished.retain(|id| *id != file_id);
        self.finished.push_back(file_id);
        while self.finished.len() > RECENT_CAP {
            self.finished.pop_front();
        }
    }

    pub fn drop_finished(&mut self, file_id: i32) {
        self.finished.retain(|id| *id != file_id);
    }

    /// Everything goes (the session closed).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.user.clear();
        self.failed.clear();
        self.requested = 0;
        self.finished.clear();
    }

    fn get(&self, file_id: i32) -> Option<&Intent> {
        self.entries.get(&file_id)
    }

    /// Any request or end is recorded for the file.
    pub fn has_record(&self, file_id: i32) -> bool {
        self.entries.contains_key(&file_id)
    }

    /// A request is out and not cancelled (paused ones included).
    pub fn is_requested(&self, file_id: i32) -> bool {
        self.get(file_id)
            .is_some_and(|i| i.phase == Phase::Requested)
    }

    pub fn is_user(&self, file_id: i32) -> bool {
        self.get(file_id)
            .is_some_and(|i| i.phase == Phase::Requested && i.origin == DownloadOrigin::User)
    }

    pub fn is_paused(&self, file_id: i32) -> bool {
        self.get(file_id)
            .is_some_and(|i| i.phase == Phase::Requested && i.paused)
    }

    /// A user download that ended without completing.
    pub fn is_failed(&self, file_id: i32) -> bool {
        self.get(file_id)
            .is_some_and(|i| i.phase == Phase::Ended && i.origin == DownloadOrigin::User)
    }

    /// An automatic download that ended; the automatic path skips it.
    pub fn is_stalled_auto(&self, file_id: i32) -> bool {
        self.get(file_id)
            .is_some_and(|i| i.phase == Phase::Ended && i.origin == DownloadOrigin::Auto)
    }

    pub fn requested_count(&self) -> usize {
        self.requested
    }

    pub fn user_ids(&self) -> impl Iterator<Item = i32> + '_ {
        self.user.iter().copied()
    }

    pub fn user_count(&self) -> usize {
        self.user.len()
    }

    /// Failed user downloads whose file is not transferring again.
    pub fn failed_ids<'a>(
        &'a self,
        files: &'a HashMap<i32, ParsedFile>,
    ) -> impl Iterator<Item = i32> + 'a {
        self.failed
            .iter()
            .copied()
            .filter(|id| self.status(*id, files) == DownloadStatus::Failed)
    }

    /// Finished user downloads, oldest first, minus the ones TDLib now
    /// reports as no longer on disk (cache cleared).
    pub fn recent<'a>(
        &'a self,
        files: &'a HashMap<i32, ParsedFile>,
    ) -> impl Iterator<Item = i32> + 'a {
        self.finished.iter().copied().filter(|id| {
            files
                .get(id)
                .is_none_or(|f| f.local.is_downloading_completed)
        })
    }

    /// The status a row shows. TDLib's file state wins: a finished file is
    /// Done whatever was asked, and a transfer TDLib reports active is
    /// Downloading unless the user cancelled it.
    pub fn status(&self, file_id: i32, files: &HashMap<i32, ParsedFile>) -> DownloadStatus {
        let local = files.get(&file_id).map(|f| &f.local);
        if local.is_some_and(|l| l.is_downloading_completed) {
            return DownloadStatus::Done;
        }
        let active = local.is_some_and(|l| l.is_downloading_active);
        match self.get(file_id) {
            Some(Intent {
                phase: Phase::Requested,
                paused: true,
                ..
            }) => DownloadStatus::Paused,
            Some(Intent {
                phase: Phase::Requested,
                ..
            }) => DownloadStatus::Downloading,
            Some(Intent {
                phase: Phase::Cancelled,
                ..
            }) => DownloadStatus::Idle,
            Some(Intent {
                phase: Phase::Ended,
                origin: DownloadOrigin::User,
                ..
            }) if !active => DownloadStatus::Failed,
            _ if active => DownloadStatus::Downloading,
            _ => DownloadStatus::Idle,
        }
    }

    /// A transfer is under way (paused ones count: the row keeps its
    /// progress).
    pub fn is_downloading(&self, file_id: i32, files: &HashMap<i32, ParsedFile>) -> bool {
        matches!(
            self.status(file_id, files),
            DownloadStatus::Downloading | DownloadStatus::Paused
        )
    }
}
