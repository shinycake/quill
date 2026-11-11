//! The one-time "about this announcement" box of a promoted chat
//! (tdesktop `HistoryWidget::showAboutTopPromotion`): when a chat that is a
//! top promotion opens and its box was not shown yet, an info toast shows
//! at the top of the conversation and goes away on its own after
//! `clamp(characters / 14, 5, 8)` seconds, or when its chat leaves the
//! window. The box is visible while the open chat is the toast's chat and
//! its time has not run out ([`PsaAbout::visible`]).
//!
//! tdesktop remembers the box as shown on the `History` object only (a
//! `seen^` prefix on the promotion type), so it comes back after a restart.
//! Quill keeps the same per-chat flag but saves it, so the box really is
//! one-time. The file is read and written on a background thread, and a
//! save merges with what is on disk, so two accounts or two processes do
//! not drop each other's flags. A chat opened before the saved flags are
//! in waits for them. The text depends on the announcement type
//! ([`PsaLabels::about`](crate::promotion::PsaLabels::about)).
#![deny(clippy::indexing_slicing)]

use crate::ids::ChatId;
use crate::locks::LockExt;
use crate::promotion::top_promotion;
use crate::state::Session;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const FILE_NAME: &str = "psa_about_seen.json";

/// Serialises file access inside the process.
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// The box on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutToast {
    pub chat_id: ChatId,
    /// Tells one showing from the next.
    pub serial: u32,
    pub expires_at: Instant,
}

/// A worker thread's result, polled without blocking. A panic in the worker
/// is caught by `spawn_guarded` and shows up as `Some(None)`, so a stage
/// that waits on a job always gets out.
#[derive(Debug, Clone)]
struct Job<T>(Arc<Mutex<Option<JoinHandle<Option<T>>>>>);

impl<T> PartialEq for Job<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl<T> Eq for Job<T> {}

impl<T: Send + 'static> Job<T> {
    /// Run `work` on a worker thread; `None` when no thread could start.
    fn spawn(name: &str, work: impl FnOnce() -> T + Send + 'static) -> Option<Self> {
        let handle = crate::thread_guard::spawn_guarded(name, work).ok()?;
        Some(Self(Arc::new(Mutex::new(Some(handle)))))
    }

    /// `None` while the worker runs; `Some(None)` when it panicked.
    fn poll(&self) -> Option<Option<T>> {
        let mut slot = self.0.lock_or_recover();
        if !slot.as_ref().is_some_and(JoinHandle::is_finished) {
            return None;
        }
        let handle = slot.take()?;
        Some(handle.join().ok().flatten())
    }

    /// Block until the worker ends (tests: a join, not a clock).
    #[cfg(test)]
    fn wait(&self) -> Option<T> {
        let handle = self.0.lock_or_recover().take()?;
        handle.join().ok().flatten()
    }
}

/// The saved flags' file, from the first read to what a save stands at.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    /// The read has not started. `waiting` is the chat opened meanwhile: it
    /// gets its box once the flags are in (if it is still the open one).
    Unread { waiting: Option<ChatId> },
    /// The read is out. `waiting` is the chat that opened meanwhile.
    Reading {
        job: Job<BTreeSet<i64>>,
        waiting: Option<ChatId>,
    },
    /// The flags are in.
    Ready {
        save: Save,
        toast: Option<AboutToast>,
    },
}

impl Default for Stage {
    fn default() -> Self {
        Self::Unread { waiting: None }
    }
}

/// Whether `seen` differs from the file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Save {
    #[default]
    InSync,
    /// A flag was added that no save has carried yet.
    Unsaved,
    /// The write of `written` is out; it ends `InSync` when it succeeded
    /// and carried everything, `Unsaved` when a flag came in meanwhile, and
    /// `Failed` when it did not succeed.
    Writing {
        job: Job<bool>,
        written: BTreeSet<i64>,
    },
    /// The last write failed. Nothing writes again on its own (a folder
    /// that cannot be written would start a worker on every sync); the
    /// next new flag saves them all.
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PsaAbout {
    seen: BTreeSet<i64>,
    stage: Stage,
    serial: u32,
    /// Overrides the app data folder (tests).
    file: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    seen: BTreeSet<i64>,
}

impl PsaAbout {
    pub fn was_shown(&self, chat_id: ChatId) -> bool {
        self.seen.contains(&chat_id.0)
    }

    pub fn is_loaded(&self) -> bool {
        matches!(self.stage, Stage::Ready { .. })
    }

    /// The box to draw: its chat is the open one and its time is not up.
    pub fn visible(&self, open_chat: Option<ChatId>, now: Instant) -> Option<&AboutToast> {
        match &self.stage {
            Stage::Ready {
                toast: Some(toast), ..
            } if open_chat == Some(toast.chat_id) && now < toast.expires_at => Some(toast),
            _ => None,
        }
    }

    /// Drop the box and anything waiting for the saved flags.
    pub fn hide(&mut self) {
        match &mut self.stage {
            Stage::Unread { waiting } | Stage::Reading { waiting, .. } => *waiting = None,
            Stage::Ready { toast, .. } => *toast = None,
        }
    }

    /// A chat opened before the saved flags were in: it waits for them.
    fn wait_for_load(&mut self, chat_id: ChatId) {
        if let Stage::Unread { waiting } | Stage::Reading { waiting, .. } = &mut self.stage {
            *waiting = Some(chat_id);
        }
    }

    /// The flags are in (`saved` merged into `seen`): the stage is ready.
    /// Returns the chat that waited for them.
    fn ready(&mut self, saved: BTreeSet<i64>) -> Option<ChatId> {
        self.seen.extend(saved);
        let (waiting, stage) = match std::mem::take(&mut self.stage) {
            Stage::Unread { waiting } | Stage::Reading { waiting, .. } => (
                waiting,
                Stage::Ready {
                    save: Save::InSync,
                    toast: None,
                },
            ),
            ready @ Stage::Ready { .. } => (None, ready),
        };
        self.stage = stage;
        waiting
    }

    /// Point the file at a folder other than the app data folder.
    pub fn use_file(&mut self, path: PathBuf) {
        self.file = Some(path);
    }

    fn file_path(&self) -> Option<PathBuf> {
        self.file
            .clone()
            .or_else(|| crate::settings::safe_app_root().map(|root| root.join(FILE_NAME)))
    }

    /// Start reading the saved flags on a worker thread, once. Without a
    /// file or a thread there is nothing to read: the flags are empty and
    /// the stage is ready now (the chat that waited is returned).
    fn start_load(&mut self) -> Option<Option<ChatId>> {
        let Stage::Unread { waiting } = self.stage else {
            return None;
        };
        let Some(path) = self.file_path() else {
            return Some(self.ready(BTreeSet::new()));
        };
        let Some(job) = Job::spawn("quill-psa-about-read", move || load_seen_from(&path)) else {
            return Some(self.ready(BTreeSet::new()));
        };
        self.stage = Stage::Reading { job, waiting };
        None
    }

    /// The flags the worker finished with: `Some(waiting chat)` when they
    /// came in just now. A worker that died leaves no flags: the box then
    /// may show once more, and nothing waits for ever.
    fn poll_load(&mut self) -> Option<Option<ChatId>> {
        let Stage::Reading { job, .. } = &self.stage else {
            return None;
        };
        let read = job.poll()?;
        Some(self.ready(read.unwrap_or_default()))
    }

    /// Without a driver (demos, tests) there is nothing to read; a chat
    /// that waited is returned.
    fn restore(&mut self, saved: BTreeSet<i64>) -> Option<ChatId> {
        self.ready(saved)
    }

    /// Settle the write that ended, then start one for flags no save has
    /// carried: on a worker thread, merged with the file. A write that
    /// failed is not started again by a sync (`Save::Failed`).
    pub fn sync_save(&mut self) {
        let ended = match &self.stage {
            Stage::Ready {
                save: Save::Writing { job, .. },
                ..
            } => match job.poll() {
                Some(result) => Some(result),
                None => return,
            },
            _ => None,
        };
        if let Some(result) = ended {
            self.settle_save(result);
        }
        let Stage::Ready { save, .. } = &mut self.stage else {
            return;
        };
        if *save != Save::Unsaved {
            return;
        }
        // Without a file there is nothing to save to: the flags stay
        // unsaved rather than counted as written.
        let Some(path) = self
            .file
            .clone()
            .or_else(|| crate::settings::safe_app_root().map(|root| root.join(FILE_NAME)))
        else {
            return;
        };
        let written = self.seen.clone();
        let flags = written.clone();
        // No thread: the flags stay unsaved and the next sync asks again.
        if let Some(job) = Job::spawn("quill-psa-about-save", move || merge_save(&path, &flags)) {
            *save = Save::Writing { job, written };
        }
    }

    /// The write ended with `result` (`None`: the worker died).
    fn settle_save(&mut self, result: Option<bool>) {
        let Stage::Ready { save, .. } = &mut self.stage else {
            return;
        };
        if let Save::Writing { written, .. } = save {
            *save = match result {
                Some(true) if *written == self.seen => Save::InSync,
                Some(true) => Save::Unsaved,
                Some(false) | None => Save::Failed,
            };
        }
    }

    /// Whether a write is out (tests: that one was, or was not, started).
    #[cfg(test)]
    fn is_writing(&self) -> bool {
        matches!(
            self.stage,
            Stage::Ready {
                save: Save::Writing { .. },
                ..
            }
        )
    }

    fn show(&mut self, chat_id: ChatId, now: Instant, lifetime: Duration) {
        let Stage::Ready { save, toast } = &mut self.stage else {
            return;
        };
        self.seen.insert(chat_id.0);
        if !matches!(save, Save::Writing { .. }) {
            *save = Save::Unsaved;
        }
        self.serial = self.serial.wrapping_add(1);
        *toast = Some(AboutToast {
            chat_id,
            serial: self.serial,
            expires_at: now + lifetime,
        });
    }
}

impl Session {
    /// `showAboutTopPromotion`, run when the main window opens a chat: the
    /// previous box goes (`_topToast.hide`), and a promoted chat whose box
    /// was not shown yet gets it, once the saved flags are known.
    pub(crate) fn show_promotion_about(&mut self, chat_id: ChatId, now: Instant) {
        self.chat_list.psa_about.hide();
        if !self.chat_list.psa_about.is_loaded() {
            self.chat_list.psa_about.wait_for_load(chat_id);
            return;
        }
        self.show_about_if_due(chat_id, now);
    }

    /// Start the read of the saved flags (once) and take them in when the
    /// worker is done: a chat opened meanwhile gets its box now.
    pub fn poll_psa_about(&mut self, now: Instant) {
        let psa = &mut self.chat_list.psa_about;
        let arrived = psa.start_load().or_else(|| psa.poll_load());
        self.show_waiting_about(arrived, now);
    }

    /// Tests: wait for the read or write that is out (a join, not a clock)
    /// and take its result in.
    #[cfg(test)]
    pub(crate) fn finish_psa_background(&mut self, now: Instant) {
        let psa = &mut self.chat_list.psa_about;
        let read = match &psa.stage {
            Stage::Reading { job, .. } => Some(job.wait()),
            _ => None,
        };
        let arrived = read.map(|read| psa.ready(read.unwrap_or_default()));
        let write = match &psa.stage {
            Stage::Ready {
                save: Save::Writing { job, .. },
                ..
            } => Some(job.wait()),
            _ => None,
        };
        if let Some(result) = write {
            psa.settle_save(result);
        }
        self.show_waiting_about(arrived, now);
    }

    /// Without a driver (demos, tests): the flags are `saved`.
    pub fn restore_psa_about(&mut self, saved: BTreeSet<i64>, now: Instant) {
        let arrived = Some(self.chat_list.psa_about.restore(saved));
        self.show_waiting_about(arrived, now);
    }

    fn show_waiting_about(&mut self, arrived: Option<Option<ChatId>>, now: Instant) {
        if let Some(Some(chat_id)) = arrived
            && self.open_chat_id() == Some(chat_id)
        {
            self.show_about_if_due(chat_id, now);
        }
    }

    fn show_about_if_due(&mut self, chat_id: ChatId, now: Instant) {
        if self.chat_list.psa_about.was_shown(chat_id) {
            return;
        }
        let Some(promotion) = self
            .chats
            .get(&chat_id.0)
            .and_then(|chat| top_promotion(self, chat))
        else {
            return;
        };
        let text = self.chat_list.psa_labels.about(promotion);
        let lifetime = toast_duration(&plain_text(&text));
        self.chat_list.psa_about.show(chat_id, now, lifetime);
    }
}

/// How long the box stays: `CountToastDuration` over the shown text, in
/// UTF-16 units like Qt's `QString::size`.
pub fn toast_duration(plain: &str) -> Duration {
    let units = u64::try_from(plain.encode_utf16().count()).unwrap_or(u64::MAX);
    Duration::from_millis((units.saturating_mul(1000) / 14).clamp(5000, 8000))
}

/// `text` split at `**` into runs; odd runs are bold. An unpaired `**`
/// stays literal.
pub fn rich_runs(text: &str) -> Vec<(String, bool)> {
    let parts: Vec<&str> = text.split("**").collect();
    if parts.len().is_multiple_of(2) {
        return vec![(text.to_string(), false)];
    }
    parts
        .into_iter()
        .enumerate()
        .filter(|(_, part)| !part.is_empty())
        .map(|(index, part)| (part.to_string(), index % 2 == 1))
        .collect()
}

/// The text as shown, without the bold markers.
pub fn plain_text(text: &str) -> String {
    rich_runs(text).into_iter().map(|(run, _)| run).collect()
}

pub fn load_seen_from(path: &Path) -> BTreeSet<i64> {
    let _guard = FILE_LOCK.lock_or_recover();
    read_file(path)
}

fn read_file(path: &Path) -> BTreeSet<i64> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
        .map(|saved| saved.seen)
        .unwrap_or_default()
}

/// Write `seen` united with what the file holds now, so a session that
/// knows fewer flags cannot take away the ones another one saved. A failed
/// save only means the box shows once more next run.
pub fn merge_save(path: &Path, seen: &BTreeSet<i64>) -> bool {
    let _guard = FILE_LOCK.lock_or_recover();
    let mut merged = read_file(path);
    merged.extend(seen.iter().copied());
    crate::settings::write_json_atomic(path, &Saved { seen: merged }).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::promotion::Promotion;
    use crate::telegram::envelope::{ChannelMemberStatus, ChatKind};

    fn session_with_promoted(id: i64) -> Session {
        let sink: std::sync::Arc<dyn crate::diagnostics::DiagnosticSink> =
            std::sync::Arc::new(crate::diagnostics::MemorySink::new());
        let mut s = Session::new(crate::ids::AccountKey::primary(), sink);
        let mut chat = crate::state::placeholder_chat(ChatId(id));
        chat.kind = ChatKind::Supergroup {
            supergroup_id: id,
            is_channel: true,
        };
        chat.assume_member_status(Some(ChannelMemberStatus::Left));
        s.chats.insert(id, chat);
        s.chat_list.promoted_chats.insert(
            id,
            Promotion::Psa {
                kind: "covid".into(),
                text: String::new(),
            },
        );
        s.restore_psa_about(BTreeSet::new(), Instant::now());
        s
    }

    fn visible(s: &Session) -> bool {
        s.chat_list
            .psa_about
            .visible(s.open_chat_id(), Instant::now())
            .is_some()
    }

    fn temp_file(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("quill-psa-{tag}-{}", std::process::id()));
        let file = dir.join(FILE_NAME);
        (dir, file)
    }

    #[test]
    fn opening_a_promoted_chat_shows_the_box_once() {
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(5));
        assert!(visible(&s));
        s.open_chat(ChatId(5));
        assert!(!visible(&s), "already shown");
        assert!(s.chat_list.psa_about.was_shown(ChatId(5)));
    }

    #[test]
    fn other_chats_and_joined_channels_get_no_box() {
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(9));
        assert!(!visible(&s));
        if let Some(chat) = s.chats.get_mut(&5) {
            chat.assume_member_status(Some(ChannelMemberStatus::Member));
        }
        s.open_chat(ChatId(5));
        assert!(!visible(&s), "useTopPromotion");
        assert!(!s.chat_list.psa_about.was_shown(ChatId(5)));
    }

    #[test]
    fn pinned_chats_and_bots_with_messages_get_no_box() {
        let mut s = session_with_promoted(5);
        if let Some(chat) = s.chats.get_mut(&5) {
            chat.is_pinned = true;
        }
        s.open_chat(ChatId(5));
        assert!(!visible(&s), "a pinned chat");
    }

    #[test]
    fn leaving_the_chat_hides_the_box() {
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(5));
        assert!(visible(&s));
        s.open_chat(ChatId(9));
        assert!(!visible(&s));
    }

    #[test]
    fn popping_the_chat_out_of_the_window_hides_the_box() {
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(5));
        assert!(visible(&s));
        assert_eq!(s.leave_open_chat(), Some(ChatId(5)));
        assert!(!visible(&s));
        s.open_chat(ChatId(5));
        assert!(!visible(&s), "it is one-time, also after coming back");
    }

    #[test]
    fn the_box_is_visible_only_until_its_time_is_up() {
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(5));
        let about = &s.chat_list.psa_about;
        let open = s.open_chat_id();
        assert!(about.visible(open, Instant::now()).is_some());
        let later = Instant::now() + Duration::from_secs(9);
        assert!(about.visible(open, later).is_none());
    }

    #[test]
    fn a_chat_opened_before_the_flags_are_in_waits_for_them() {
        let (dir, file) = temp_file("wait");
        merge_save(&file, &BTreeSet::from([5]));
        let mut s = session_with_promoted(5);
        s.chat_list.psa_about = PsaAbout::default();
        s.chat_list.psa_about.use_file(file.clone());
        s.open_chat(ChatId(5));
        assert!(!visible(&s), "not before the flags are known");
        s.poll_psa_about(Instant::now());
        assert!(!s.chat_list.psa_about.is_loaded(), "the read is out");
        s.finish_psa_background(Instant::now());
        assert!(s.chat_list.psa_about.is_loaded());
        assert!(!visible(&s), "the saved flag says it was shown");

        let mut fresh = session_with_promoted(6);
        fresh.chat_list.psa_about = PsaAbout::default();
        fresh.chat_list.psa_about.use_file(dir.join("none.json"));
        fresh.open_chat(ChatId(6));
        fresh.poll_psa_about(Instant::now());
        fresh.finish_psa_background(Instant::now());
        assert!(visible(&fresh), "unseen: the box shows once they are in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_flag_survives_a_restart() {
        let (dir, file) = temp_file("restart");
        let mut s = session_with_promoted(5);
        s.chat_list.psa_about.use_file(file.clone());
        s.open_chat(ChatId(5));
        s.chat_list.psa_about.sync_save();
        s.finish_psa_background(Instant::now());
        assert!(load_seen_from(&file).contains(&5));

        let mut restarted = session_with_promoted(5);
        restarted.chat_list.psa_about = PsaAbout::default();
        restarted.chat_list.psa_about.use_file(file.clone());
        restarted.poll_psa_about(Instant::now());
        restarted.finish_psa_background(Instant::now());
        restarted.open_chat(ChatId(5));
        assert!(!visible(&restarted));

        std::fs::write(&file, b"{").ok();
        assert!(load_seen_from(&file).is_empty(), "a broken file is empty");
        assert!(load_seen_from(&dir.join("none")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_sessions_saving_do_not_overwrite_each_other() {
        let (dir, file) = temp_file("two");
        let mut a = session_with_promoted(1);
        let mut b = session_with_promoted(2);
        a.chat_list.psa_about.use_file(file.clone());
        b.chat_list.psa_about.use_file(file.clone());
        a.open_chat(ChatId(1));
        b.open_chat(ChatId(2));
        a.chat_list.psa_about.sync_save();
        a.finish_psa_background(Instant::now());
        assert!(load_seen_from(&file).contains(&1));
        b.chat_list.psa_about.sync_save();
        b.finish_psa_background(Instant::now());
        assert!(load_seen_from(&file).contains(&2));
        assert_eq!(load_seen_from(&file), BTreeSet::from([1, 2]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_flags_are_saved_once_after_they_change() {
        let (dir, file) = temp_file("once");
        let mut s = session_with_promoted(5);
        s.chat_list.psa_about.use_file(file.clone());
        s.chat_list.psa_about.sync_save();
        assert!(!s.chat_list.psa_about.is_writing(), "nothing changed");
        s.open_chat(ChatId(5));
        s.chat_list.psa_about.sync_save();
        assert!(s.chat_list.psa_about.is_writing());
        s.finish_psa_background(Instant::now());
        assert!(file.exists());
        s.chat_list.psa_about.sync_save();
        assert!(!s.chat_list.psa_about.is_writing(), "not saved again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A write that fails is not counted as written, and no sync starts it
    /// again on its own: the next new flag saves them all.
    #[test]
    fn a_failed_write_waits_for_the_next_flag() {
        let (dir, file) = temp_file("retry");
        std::fs::create_dir_all(&dir).ok();
        // A regular file where the folder should be: every write fails.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"x").ok();
        let mut s = session_with_promoted(5);
        s.chat_list.psa_about.use_file(blocker.join(FILE_NAME));
        s.open_chat(ChatId(5));
        s.chat_list.psa_about.sync_save();
        s.finish_psa_background(Instant::now());
        for _ in 0..3 {
            s.chat_list.psa_about.sync_save();
            assert!(
                !s.chat_list.psa_about.is_writing(),
                "a failed write is not started again by a sync"
            );
        }
        // The folder works again and another box is shown: one write
        // carries both flags.
        s.chat_list.psa_about.use_file(file.clone());
        s.chat_list
            .psa_about
            .show(ChatId(6), Instant::now(), Duration::from_secs(5));
        s.chat_list.psa_about.sync_save();
        assert!(s.chat_list.psa_about.is_writing());
        s.finish_psa_background(Instant::now());
        assert_eq!(load_seen_from(&file), BTreeSet::from([5, 6]));
        s.chat_list.psa_about.sync_save();
        assert!(!s.chat_list.psa_about.is_writing(), "now in sync");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `restore` (a demo, a test) resolves the chat that waited: nothing is
    /// left waiting with no one to resolve it.
    #[test]
    fn restoring_the_flags_shows_the_box_of_the_chat_that_waited() {
        let mut s = session_with_promoted(5);
        s.chat_list.psa_about = PsaAbout::default();
        s.chat_list.psa_about.use_file(temp_file("restore").1);
        s.open_chat(ChatId(5));
        assert!(!visible(&s));
        s.restore_psa_about(BTreeSet::new(), Instant::now());
        assert!(visible(&s));
    }

    #[test]
    fn duration_follows_the_shown_text_length() {
        assert_eq!(toast_duration("short"), Duration::from_secs(5));
        assert_eq!(toast_duration(&"x".repeat(98)), Duration::from_secs(7));
        assert_eq!(toast_duration(&"x".repeat(400)), Duration::from_secs(8));
        let marked = format!("{} **{}**", "x".repeat(60), "y".repeat(10));
        assert_eq!(plain_text(&marked).chars().count(), 71);
        assert_eq!(
            toast_duration(&plain_text(&marked)),
            Duration::from_millis(5071),
            "the markers are not counted"
        );
        assert_eq!(toast_duration("😀".repeat(40).as_str()).as_millis(), 5714);
    }

    #[test]
    fn bold_runs_split_at_double_stars() {
        assert_eq!(
            rich_runs("a **b** c"),
            vec![
                ("a ".into(), false),
                ("b".into(), true),
                (" c".into(), false)
            ]
        );
        assert_eq!(rich_runs("plain"), vec![("plain".into(), false)]);
        assert_eq!(rich_runs("a ** b"), vec![("a ** b".into(), false)]);
        assert_eq!(plain_text("a **b** c"), "a b c");
    }
}
