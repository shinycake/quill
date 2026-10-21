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
use crate::promotion::top_promotion;
use crate::state::Session;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

/// Where the background read leaves the saved flags.
#[derive(Debug, Clone, Default)]
struct LoadSlot(Arc<Mutex<Option<BTreeSet<i64>>>>);

impl PartialEq for LoadSlot {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for LoadSlot {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Load {
    #[default]
    Idle,
    Reading(LoadSlot),
    Done,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PsaAbout {
    seen: BTreeSet<i64>,
    load: Load,
    /// `seen` changed since it was last saved.
    dirty: bool,
    toast: Option<AboutToast>,
    serial: u32,
    /// A chat opened before the saved flags were in.
    waiting: Option<ChatId>,
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
        self.load == Load::Done
    }

    /// The box to draw: its chat is the open one and its time is not up.
    pub fn visible(&self, open_chat: Option<ChatId>, now: Instant) -> Option<&AboutToast> {
        self.toast
            .as_ref()
            .filter(|toast| open_chat == Some(toast.chat_id) && now < toast.expires_at)
    }

    /// Drop the box and anything waiting for the saved flags.
    pub fn hide(&mut self) {
        self.toast = None;
        self.waiting = None;
    }

    /// Without a driver (demos, tests) there is nothing to read.
    pub fn restore(&mut self, saved: BTreeSet<i64>) {
        self.seen.extend(saved);
        self.load = Load::Done;
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

    /// Start reading the saved flags on a background thread, once.
    pub fn start_load(&mut self) {
        if self.load != Load::Idle {
            return;
        }
        let Some(path) = self.file_path() else {
            self.load = Load::Done;
            return;
        };
        let slot = LoadSlot::default();
        let out = slot.0.clone();
        self.load = Load::Reading(slot);
        std::thread::spawn(move || {
            let seen = load_seen_from(&path);
            if let Ok(mut guard) = out.lock() {
                *guard = Some(seen);
            }
        });
    }

    /// Take the flags the background read finished with. `true` when they
    /// came in just now.
    pub fn poll_load(&mut self) -> bool {
        let Load::Reading(slot) = &self.load else {
            return false;
        };
        let read = slot.0.lock().ok().and_then(|mut guard| guard.take());
        let Some(read) = read else {
            return false;
        };
        self.restore(read);
        true
    }

    /// Save the flags that changed, merged with the file, on a background
    /// thread.
    pub fn save_unsaved(&mut self) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        let Some(path) = self.file_path() else {
            return;
        };
        let seen = self.seen.clone();
        std::thread::spawn(move || merge_save(&path, &seen));
    }

    fn show(&mut self, chat_id: ChatId, now: Instant, lifetime: Duration) {
        self.seen.insert(chat_id.0);
        self.dirty = true;
        self.serial = self.serial.wrapping_add(1);
        self.toast = Some(AboutToast {
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
            self.chat_list.psa_about.waiting = Some(chat_id);
            return;
        }
        self.show_about_if_due(chat_id, now);
    }

    /// The saved flags arrived: a chat opened meanwhile gets its box now.
    pub fn poll_psa_about(&mut self, now: Instant) {
        if !self.chat_list.psa_about.poll_load() {
            return;
        }
        if let Some(chat_id) = self.chat_list.psa_about.waiting.take()
            && self.open_chat == Some(chat_id)
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
    let _guard = FILE_LOCK.lock();
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
pub fn merge_save(path: &Path, seen: &BTreeSet<i64>) {
    let _guard = FILE_LOCK.lock();
    let mut merged = read_file(path);
    merged.extend(seen.iter().copied());
    let _ = crate::settings::write_json_atomic(path, &Saved { seen: merged });
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
        chat.my_member_status = Some(ChannelMemberStatus::Left);
        s.chats.insert(id, chat);
        s.chat_list.promoted_chats.insert(
            id,
            Promotion::Psa {
                kind: "covid".into(),
                text: String::new(),
            },
        );
        s.chat_list.psa_about.restore(BTreeSet::new());
        s
    }

    fn visible(s: &Session) -> bool {
        s.chat_list
            .psa_about
            .visible(s.open_chat, Instant::now())
            .is_some()
    }

    fn temp_file(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("quill-psa-{tag}-{}", std::process::id()));
        let file = dir.join(FILE_NAME);
        (dir, file)
    }

    fn wait_for(mut done: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
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
            chat.my_member_status = Some(ChannelMemberStatus::Member);
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
        let open = s.open_chat;
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
        s.chat_list.psa_about.start_load();
        assert!(wait_for(|| {
            s.poll_psa_about(Instant::now());
            s.chat_list.psa_about.is_loaded()
        }));
        assert!(!visible(&s), "the saved flag says it was shown");

        let mut fresh = session_with_promoted(6);
        fresh.chat_list.psa_about = PsaAbout::default();
        fresh.chat_list.psa_about.use_file(dir.join("none.json"));
        fresh.open_chat(ChatId(6));
        fresh.chat_list.psa_about.start_load();
        assert!(wait_for(|| {
            fresh.poll_psa_about(Instant::now());
            fresh.chat_list.psa_about.is_loaded()
        }));
        assert!(visible(&fresh), "unseen: the box shows once they are in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_flag_survives_a_restart() {
        let (dir, file) = temp_file("restart");
        let mut s = session_with_promoted(5);
        s.chat_list.psa_about.use_file(file.clone());
        s.open_chat(ChatId(5));
        s.chat_list.psa_about.save_unsaved();
        assert!(wait_for(|| load_seen_from(&file).contains(&5)));

        let mut restarted = session_with_promoted(5);
        restarted.chat_list.psa_about = PsaAbout::default();
        restarted.chat_list.psa_about.use_file(file.clone());
        restarted.chat_list.psa_about.start_load();
        assert!(wait_for(|| {
            restarted.poll_psa_about(Instant::now());
            restarted.chat_list.psa_about.is_loaded()
        }));
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
        a.chat_list.psa_about.save_unsaved();
        assert!(wait_for(|| load_seen_from(&file).contains(&1)));
        b.chat_list.psa_about.save_unsaved();
        assert!(wait_for(|| load_seen_from(&file).contains(&2)));
        assert_eq!(load_seen_from(&file), BTreeSet::from([1, 2]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_flags_are_saved_once_after_they_change() {
        let (dir, file) = temp_file("once");
        let mut s = session_with_promoted(5);
        s.chat_list.psa_about.use_file(file.clone());
        s.chat_list.psa_about.save_unsaved();
        std::thread::sleep(Duration::from_millis(50));
        assert!(!file.exists(), "nothing changed, nothing written");
        s.open_chat(ChatId(5));
        s.chat_list.psa_about.save_unsaved();
        assert!(wait_for(|| file.exists()));
        let _ = std::fs::remove_file(&file);
        s.chat_list.psa_about.save_unsaved();
        std::thread::sleep(Duration::from_millis(50));
        assert!(!file.exists(), "not saved again");
        let _ = std::fs::remove_dir_all(&dir);
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
