//! The one-time "about this announcement" box of a promoted chat
//! (tdesktop `HistoryWidget::showAboutTopPromotion`): when a chat that is a
//! top promotion opens and its box was not shown yet, an info toast slides
//! in at the top of the conversation and goes away on its own after
//! `clamp(characters / 14, 5, 8)` seconds, or when another chat opens.
//!
//! tdesktop remembers the box as shown on the `History` object only (a
//! `seen^` prefix on the promotion type), so it comes back after a restart.
//! Quill keeps the same per-chat flag but saves it, so the box really is
//! one-time. The text depends on the announcement type
//! ([`PsaLabels::about`](crate::promotion::PsaLabels::about)).
#![deny(clippy::indexing_slicing)]

use crate::ids::ChatId;
use crate::promotion::top_promotion;
use crate::state::Session;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const FILE_NAME: &str = "psa_about_seen.json";

/// The box on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutToast {
    pub chat_id: ChatId,
    /// Tells one showing from the next, so a late timer of an earlier
    /// showing cannot dismiss this one.
    pub serial: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PsaAbout {
    seen: BTreeSet<i64>,
    /// The saved flags were merged in.
    loaded: bool,
    /// `seen` changed since it was last saved.
    dirty: bool,
    toast: Option<AboutToast>,
    serial: u32,
    /// The showing a timer was started for.
    armed: Option<u32>,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    seen: BTreeSet<i64>,
}

impl PsaAbout {
    pub fn toast(&self) -> Option<&AboutToast> {
        self.toast.as_ref()
    }

    pub fn was_shown(&self, chat_id: ChatId) -> bool {
        self.seen.contains(&chat_id.0)
    }

    /// Drop the box (another chat opened, or its time is up).
    pub fn hide(&mut self) {
        self.toast = None;
    }

    /// The timer of showing `serial` ran out.
    pub fn expire(&mut self, serial: u32) {
        if self.toast.as_ref().is_some_and(|t| t.serial == serial) {
            self.toast = None;
        }
    }

    /// The showing a timer is still to be started for: answers
    /// `Some(serial)` once per showing.
    pub fn arm(&mut self) -> Option<u32> {
        let serial = self.toast.as_ref()?.serial;
        if self.armed == Some(serial) {
            return None;
        }
        self.armed = Some(serial);
        Some(serial)
    }

    pub fn needs_load(&self) -> bool {
        !self.loaded
    }

    /// Merge the flags saved by an earlier run.
    pub fn restore(&mut self, saved: BTreeSet<i64>) {
        self.seen.extend(saved);
        self.loaded = true;
    }

    /// The flags to save, once after they changed.
    pub fn take_unsaved(&mut self) -> Option<BTreeSet<i64>> {
        std::mem::take(&mut self.dirty).then(|| self.seen.clone())
    }

    fn show(&mut self, chat_id: ChatId) {
        self.seen.insert(chat_id.0);
        self.dirty = true;
        self.serial = self.serial.wrapping_add(1);
        self.toast = Some(AboutToast {
            chat_id,
            serial: self.serial,
        });
    }
}

impl Session {
    /// `showAboutTopPromotion`, run when the main window opens a chat: the
    /// previous box goes (`_topToast.hide`), and a promoted chat whose box
    /// was not shown yet gets it.
    pub(crate) fn show_promotion_about(&mut self, chat_id: ChatId) {
        self.chat_list.psa_about.hide();
        if self.open_topic.is_some() || self.chat_list.psa_about.was_shown(chat_id) {
            return;
        }
        let promoted = self
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| top_promotion(self, chat).is_some());
        if promoted {
            self.chat_list.psa_about.show(chat_id);
        }
    }
}

/// How long the box stays: `CountToastDuration`.
pub fn toast_duration(text: &str) -> Duration {
    let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
    Duration::from_millis((chars.saturating_mul(1000) / 14).clamp(5000, 8000))
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

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join(FILE_NAME))
}

pub fn load_seen_from(path: &Path) -> BTreeSet<i64> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
        .map(|saved| saved.seen)
        .unwrap_or_default()
}

pub fn save_seen_to(path: &Path, seen: &BTreeSet<i64>) -> std::io::Result<()> {
    crate::settings::write_json_atomic(path, &Saved { seen: seen.clone() })
}

/// The saved flags, empty when there are none or the file is broken.
pub fn load_seen() -> BTreeSet<i64> {
    path().map(|p| load_seen_from(&p)).unwrap_or_default()
}

/// A failed save only means the box shows once more next run.
pub fn save_seen(seen: &BTreeSet<i64>) {
    if let Some(path) = path() {
        let _ = save_seen_to(&path, seen);
    }
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
        s
    }

    #[test]
    fn opening_a_promoted_chat_shows_the_box_once() {
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(5));
        let toast = s.chat_list.psa_about.toast().cloned();
        assert_eq!(toast.map(|t| t.chat_id), Some(ChatId(5)));
        s.open_chat(ChatId(5));
        assert_eq!(s.chat_list.psa_about.toast(), None, "already shown");
        assert!(s.chat_list.psa_about.was_shown(ChatId(5)));
    }

    #[test]
    fn other_chats_and_joined_channels_get_no_box() {
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(9));
        assert_eq!(s.chat_list.psa_about.toast(), None);
        if let Some(chat) = s.chats.get_mut(&5) {
            chat.my_member_status = Some(ChannelMemberStatus::Member);
        }
        s.open_chat(ChatId(5));
        assert_eq!(s.chat_list.psa_about.toast(), None, "useTopPromotion");
        assert!(!s.chat_list.psa_about.was_shown(ChatId(5)));
    }

    #[test]
    fn leaving_the_chat_hides_the_box() {
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(5));
        assert!(s.chat_list.psa_about.toast().is_some());
        s.open_chat(ChatId(9));
        assert_eq!(s.chat_list.psa_about.toast(), None);
    }

    #[test]
    fn a_late_timer_cannot_dismiss_a_newer_box() {
        let mut about = PsaAbout::default();
        about.show(ChatId(1));
        assert_eq!(about.arm(), Some(1));
        assert_eq!(about.arm(), None, "armed once");
        about.show(ChatId(2));
        about.expire(1);
        assert!(about.toast().is_some());
        assert_eq!(about.arm(), Some(2));
        about.expire(2);
        assert_eq!(about.toast(), None);
    }

    #[test]
    fn the_flag_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("quill-psa-about-{}", std::process::id()));
        let file = dir.join(FILE_NAME);
        let mut s = session_with_promoted(5);
        s.open_chat(ChatId(5));
        let unsaved = s.chat_list.psa_about.take_unsaved();
        assert_eq!(s.chat_list.psa_about.take_unsaved(), None, "saved once");
        assert!(save_seen_to(&file, &unsaved.unwrap_or_default()).is_ok());

        let mut restarted = session_with_promoted(5);
        assert!(restarted.chat_list.psa_about.needs_load());
        restarted.chat_list.psa_about.restore(load_seen_from(&file));
        assert!(!restarted.chat_list.psa_about.needs_load());
        restarted.open_chat(ChatId(5));
        assert_eq!(restarted.chat_list.psa_about.toast(), None);

        std::fs::write(&file, b"{").ok();
        assert!(load_seen_from(&file).is_empty(), "a broken file is empty");
        assert!(load_seen_from(&dir.join("none")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duration_follows_the_text_length() {
        assert_eq!(toast_duration("short"), Duration::from_secs(5));
        assert_eq!(toast_duration(&"x".repeat(98)), Duration::from_secs(7));
        assert_eq!(toast_duration(&"x".repeat(400)), Duration::from_secs(8));
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
    }
}
