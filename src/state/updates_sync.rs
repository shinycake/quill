//! State behind the account-level sync updates: per-chat silent default,
//! download list totals, dice emoji list, account freeze, free speech
//! recognition quota, running live locations and age verification.
//! Pure state and copy; the UI reads it and the reducer feeds it.
use super::*;
use crate::ids::{ChatId, MessageId};
use crate::local_time::{civil_at, day_label};
use crate::telegram::envelope::{
    ActiveLiveShare, AgeVerificationParams, FreezeStateUpdate, ParsedFileDownload,
    SpeechTrialUpdate,
};

/// `updateFileDownloads`: totals of the whole download list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadTotals {
    pub total_size: i64,
    pub total_count: i32,
    pub downloaded_size: i64,
}

/// The account is frozen: read-only until an appeal succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreezeInfo {
    pub freezing_date: i32,
    pub deletion_date: i32,
    pub appeal_link: String,
}

/// A running live location of ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveShare {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub expires_at: i64,
    /// Someone opened it (`updateMessageLiveLocationViewed`).
    pub viewed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UpdatesSync {
    default_silent: HashSet<i64>,
    pub download_totals: Option<DownloadTotals>,
    /// File names of downloads that arrived through the list sync, for
    /// files whose message is not in a loaded history.
    pub download_names: HashMap<i32, String>,
    pub dice_emojis: Vec<String>,
    pub freeze: Option<FreezeInfo>,
    pub speech_trial: Option<SpeechTrialUpdate>,
    pub live_shares: Vec<LiveShare>,
    pub age_verification: Option<AgeVerificationParams>,
}

/// Dice emoji Telegram supports when the server has not sent a list yet.
pub const DEFAULT_DICE_EMOJIS: [&str; 6] = [
    "\u{1F3B2}",
    "\u{1F3AF}",
    "\u{1F3C0}",
    "\u{26BD}",
    "\u{1F3B3}",
    "\u{1F3B0}",
];

impl UpdatesSync {
    pub fn set_default_silent(&mut self, chat_id: i64, silent: bool) {
        if silent {
            self.default_silent.insert(chat_id);
        } else {
            self.default_silent.remove(&chat_id);
        }
    }

    /// `chat.default_disable_notification`: sends to this chat are silent
    /// unless the user chose otherwise.
    pub fn is_default_silent(&self, chat_id: i64) -> bool {
        self.default_silent.contains(&chat_id)
    }

    pub fn set_download_totals(&mut self, totals: DownloadTotals) {
        self.download_totals = (totals.total_count > 0).then_some(totals);
    }

    /// "3 files · 12.4 of 40.0 MB" style summary for the panel header.
    pub fn download_summary(&self, format_bytes: impl Fn(i64) -> String) -> Option<String> {
        let totals = self.download_totals?;
        let noun = if totals.total_count == 1 {
            "file"
        } else {
            "files"
        };
        let mut text = format!("{} {noun}", totals.total_count);
        if totals.total_size > 0 {
            text.push_str(&format!(
                " \u{b7} {} of {}",
                format_bytes(totals.downloaded_size.min(totals.total_size)),
                format_bytes(totals.total_size)
            ));
        }
        Some(text)
    }

    pub fn set_dice_emojis(&mut self, emojis: Vec<String>) {
        self.dice_emojis = emojis;
    }

    /// The emoji the send menu offers: the server list, or the known set
    /// until the server has sent one.
    pub fn dice_menu(&self) -> Vec<String> {
        if self.dice_emojis.is_empty() {
            DEFAULT_DICE_EMOJIS.iter().map(|e| e.to_string()).collect()
        } else {
            self.dice_emojis.clone()
        }
    }

    pub fn set_freeze(&mut self, state: FreezeStateUpdate) {
        self.freeze = state.is_frozen.then_some(FreezeInfo {
            freezing_date: state.freezing_date,
            deletion_date: state.deletion_date,
            appeal_link: state.appeal_link,
        });
    }

    pub fn set_speech_trial(&mut self, trial: SpeechTrialUpdate) {
        self.speech_trial = Some(trial);
    }

    /// Replace the running shares; ones we already knew keep their
    /// "viewed" mark.
    pub fn set_live_shares(&mut self, shares: Vec<ActiveLiveShare>) {
        let old = std::mem::take(&mut self.live_shares);
        self.live_shares = shares
            .into_iter()
            .map(|share| LiveShare {
                chat_id: share.chat_id,
                message_id: share.message_id,
                expires_at: share.expires_at,
                viewed: old.iter().any(|o| {
                    o.viewed && o.chat_id == share.chat_id && o.message_id == share.message_id
                }),
            })
            .collect();
    }

    pub fn mark_live_viewed(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(share) = self
            .live_shares
            .iter_mut()
            .find(|s| s.chat_id == chat_id && s.message_id == message_id)
        {
            share.viewed = true;
        }
    }

    /// Shares still running at `now`.
    pub fn live_shares_at(&self, now: i64) -> impl Iterator<Item = &LiveShare> {
        self.live_shares.iter().filter(move |s| s.expires_at > now)
    }

    pub fn set_age_verification(&mut self, parameters: Option<AgeVerificationParams>) {
        self.age_verification = parameters;
    }
}

/// Whether the next send is silent: the manual toggle, or the chat's
/// default unless the user switched it off for this chat.
pub fn effective_silent(manual: bool, chat_default: bool, loud_override: bool) -> bool {
    manual || (chat_default && !loud_override)
}

/// Turning on "Show 18+ Content" needs the age verification prompt while
/// the server asks for one and the user has not started it yet.
pub fn age_gate_blocks(turning_on: bool, verification_needed: bool, started: bool) -> bool {
    turning_on && verification_needed && !started
}

/// "Sharing live location in 2 chats" for the strip above the chat.
pub fn live_strip_label(count: usize) -> String {
    match count {
        0 => String::new(),
        1 => "Sharing your live location".to_string(),
        n => format!("Sharing your live location in {n} chats"),
    }
}

/// Time left of a share as "42 min left" / "1 h 05 min left" / "No time limit".
pub fn live_left_label(expires_at: i64, now: i64) -> String {
    if expires_at == i64::MAX {
        return "No time limit".into();
    }
    let left = (expires_at - now).max(0);
    if left >= 3600 {
        format!("{} h {:02} min left", left / 3600, (left % 3600) / 60)
    } else if left >= 60 {
        format!("{} min left", (left + 59) / 60)
    } else {
        format!("{left} s left")
    }
}

/// tdesktop `lng_audio_transcribe_trials_left` / `_trials_over` text for
/// the free voice-to-text quota.
pub fn speech_trial_hint(trial: &SpeechTrialUpdate, now: i64) -> String {
    let date = day_label(
        &civil_at(i64::from(trial.next_reset_date), 0),
        &civil_at(now, 0),
    );
    if trial.left_count > 0 {
        let noun = if trial.left_count == 1 {
            "transcription"
        } else {
            "transcriptions"
        };
        format!(
            "You have {} free {noun} left until {date}.",
            trial.left_count
        )
    } else {
        format!(
            "You have used all your free transcriptions this week. Wait until {date} to use it again or subscribe to Premium now."
        )
    }
}

/// The frozen-account appeal deadline as a date.
pub fn freeze_deadline_label(info: &FreezeInfo, now: i64) -> String {
    day_label(
        &civil_at(i64::from(info.deletion_date), 0),
        &civil_at(now, 0),
    )
}

/// "You must be 18 or over" copy for the age verification dialog.
pub fn age_verification_about(params: &AgeVerificationParams) -> String {
    if params.min_age > 0 {
        format!(
            "To view this content you need to confirm you are {} or older.",
            params.min_age
        )
    } else {
        "To view this content you need to confirm your age.".to_string()
    }
}

/// `t.me` link of the verification bot, or `None` when the name is empty.
pub fn age_verification_bot_url(params: &AgeVerificationParams) -> Option<String> {
    let name = params.verification_bot_username.trim_start_matches('@');
    (!name.is_empty()).then(|| format!("https://t.me/{name}"))
}

impl Session {
    /// A download that appeared in the list, possibly started on another
    /// device: track it like one started here.
    pub(crate) fn apply_download_added(&mut self, download: ParsedFileDownload) {
        self.remember_files(&download.files);
        let id = download.file_id;
        if let Some(name) = download.name {
            self.sync.download_names.insert(id, name);
        }
        self.failed_downloads.remove(&id);
        if download.complete_date != 0 {
            self.user_downloads.insert(id);
            self.record_completed_user_download(id);
            self.user_downloads.remove(&id);
            self.paused_downloads.remove(&id);
        } else {
            self.user_downloads.insert(id);
            if download.is_paused {
                self.paused_downloads.insert(id);
            } else {
                self.paused_downloads.remove(&id);
            }
        }
    }

    /// The download left the list (removed here or on another device).
    pub(crate) fn apply_download_removed(&mut self, file_id: i32) {
        self.user_downloads.remove(&file_id);
        self.paused_downloads.remove(&file_id);
        self.failed_downloads.remove(&file_id);
        self.completed_downloads.retain(|id| *id != file_id);
        self.sync.download_names.remove(&file_id);
    }

    /// Whether sends should be refused locally: a frozen account is read-only.
    pub fn is_frozen(&self) -> bool {
        self.sync.freeze.is_some()
    }
}
