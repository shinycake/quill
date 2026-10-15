//! The transfer picker of a collectible and its export to the TON
//! blockchain through Fragment (tdesktop `boxes/transfer_gift_box.cpp`:
//! `ShowTransferGiftBox`, `MakeExportOption`, `ConfirmExportBox`,
//! `ExportOnBlockchain`, `ShowTransferToBox`, `ShowActionLocked`).
//!
//! Pure core: the stages of the picker, the unlock countdown of the "Send
//! via Blockchain" row and the people the picker lists. Nothing here sends
//! a request; the driver is `connect::gift_manage`.

use crate::premium_hub::ReceivedGift;

/// Seconds in an hour, and the half hour tdesktop rounds by.
const HOUR: i64 = 3_600;
const HALF_HOUR: i64 = 1_800;

/// Telegram's service notifications account; tdesktop's contacts list
/// skips it (`isServiceUser`).
const SERVICE_USER_ID: i64 = 777_000;

/// Whether the "Send via Blockchain" row can be used now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportState {
    /// `export_date == 0`: the gift can't leave Telegram; no row.
    Unavailable,
    /// The row is shown with "unlocks in ..."; tapping it explains.
    Locked { hours: i64 },
    /// Tapping the row asks to open Fragment.
    Ready,
}

impl ExportState {
    /// tdesktop `MakeExportOption` / `ExportRow`: the hours left are
    /// `max((left + 1800) / 3600, 1)` while `export_date` is in the future.
    pub fn of(export_date: i64, now: i64) -> Self {
        if export_date <= 0 {
            return Self::Unavailable;
        }
        let left = export_date.saturating_sub(now);
        if left <= 0 {
            Self::Ready
        } else {
            Self::Locked {
                hours: unlock_hours(left),
            }
        }
    }
}

/// tdesktop's rounding of the time left: to the nearest hour, at least one.
pub fn unlock_hours(left: i64) -> i64 {
    (left.saturating_add(HALF_HOUR) / HOUR).max(1)
}

/// How the countdown reads: whole days once there is at least one
/// (`days = hours / 24`), hours otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockIn {
    Days(i64),
    Hours(i64),
}

impl UnlockIn {
    pub fn of(hours: i64) -> Self {
        let days = hours / 24;
        if days > 0 {
            Self::Days(days)
        } else {
            Self::Hours(hours.max(1))
        }
    }
}

/// A wrong or refused password in the password box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// `PASSWORD_HASH_INVALID`: "Wrong cloud password".
    WrongPassword,
    /// A flood wait: "Too many tries. Please try again later."
    Flood,
}

/// Where the picker is. The picker is one dialog; each later step replaces
/// its content, and Cancel on a step goes back to `Pick`, the way
/// tdesktop's boxes stack over the contacts list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStage {
    /// "Transfer {name}": the search, "Send via Blockchain" and contacts.
    Pick,
    /// "Do you want to transfer ownership of {name} to {recipient}?"
    ConfirmPeer { user_id: i64, name: String },
    /// "Unlocking in progress".
    Locked { hours: i64 },
    /// "Manage with Fragment" with "Open Fragment".
    ConfirmExport,
    /// "Two-step verification": the password box. `sending` while the
    /// request is out; the password itself is never kept here.
    Password {
        sending: bool,
        error: Option<ExportError>,
    },
    /// "Security check" (tdesktop `TransferPasswordError`): no password
    /// set (`later: false`) or set too recently (`later: true`).
    SecurityCheck { later: bool },
    /// "Action Locked": the gift sits in a TON wallet, not in Telegram.
    ActionLocked,
}

/// The open transfer picker of one received collectible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferPicker {
    /// `received_gift_id`.
    pub id: String,
    /// "Plush Pepe #1,204" (tdesktop `UniqueGiftName`).
    pub gift_name: String,
    /// The collectible's `name` slug, for Fragment's gift page.
    pub slug: String,
    pub stage: TransferStage,
}

impl TransferPicker {
    pub fn new(
        id: impl Into<String>,
        gift_name: impl Into<String>,
        slug: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            gift_name: gift_name.into(),
            slug: slug.into(),
            stage: TransferStage::Pick,
        }
    }

    /// The picker for one of the user's collectibles; "Action Locked" when
    /// the gift sits in a TON wallet (tdesktop's Transfer menu item).
    pub fn for_gift(gift: &ReceivedGift) -> Self {
        let (name, slug) = match gift.gift.collectible.as_deref() {
            Some(c) => (c.full_name(), c.name.clone()),
            None => (gift.gift.title.clone(), String::new()),
        };
        let mut picker = Self::new(gift.id.clone(), name, slug);
        if gift.held_outside_telegram() {
            picker.stage = TransferStage::ActionLocked;
        }
        picker
    }

    /// Whether the export request is out (the buttons wait).
    pub fn exporting(&self) -> bool {
        matches!(self.stage, TransferStage::Password { sending: true, .. })
    }

    /// A contact was picked: ask before giving the gift away.
    pub fn choose(&mut self, user_id: i64, name: impl Into<String>) {
        if self.stage == TransferStage::Pick {
            self.stage = TransferStage::ConfirmPeer {
                user_id,
                name: name.into(),
            };
        }
    }

    /// "Send via Blockchain" was tapped: explain the wait, or ask to open
    /// Fragment (tdesktop `MakeExportOption`'s `activate`).
    pub fn tap_export(&mut self, state: ExportState) {
        if self.stage != TransferStage::Pick {
            return;
        }
        match state {
            ExportState::Unavailable => {}
            ExportState::Locked { hours } => self.stage = TransferStage::Locked { hours },
            ExportState::Ready => self.stage = TransferStage::ConfirmExport,
        }
    }

    /// "Open Fragment" in the confirmation: the password box.
    pub fn open_fragment(&mut self) {
        if self.stage == TransferStage::ConfirmExport {
            self.stage = TransferStage::Password {
                sending: false,
                error: None,
            };
        }
    }

    /// Cancel or OK on a step: back to the list. Not while the export
    /// request is out.
    pub fn back(&mut self) {
        if !self.exporting() {
            self.stage = TransferStage::Pick;
        }
    }
}

/// tdesktop `ShowActionLocked`: Fragment's page of the gift in the
/// owner's collection.
pub fn fragment_gift_url(slug: &str) -> String {
    format!("https://fragment.com/gift/{slug}?collection=my")
}

/// One person in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferCandidate {
    pub user_id: i64,
    pub name: String,
    pub status: String,
}

/// tdesktop `Controller::createRow`: contacts, without the user, bots,
/// the service account and deleted accounts (no name), filtered by the
/// search words, in the order given (the contacts list is by name).
pub fn transfer_candidates<'a>(
    rows: impl IntoIterator<Item = (i64, &'a str, &'a str, bool)>,
    me: Option<i64>,
    query: &str,
) -> Vec<TransferCandidate> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    rows.into_iter()
        .filter(|(user_id, name, _, is_bot)| {
            *user_id > 0
                && Some(*user_id) != me
                && *user_id != SERVICE_USER_ID
                && !*is_bot
                && !name.trim().is_empty()
        })
        .filter(|(_, name, _, _)| {
            let lower = name.to_lowercase();
            let parts: Vec<&str> = lower.split_whitespace().collect();
            words
                .iter()
                .all(|word| parts.iter().any(|part| part.starts_with(word.as_str())))
        })
        .map(|(user_id, name, status, _)| TransferCandidate {
            user_id,
            name: name.to_string(),
            status: status.to_string(),
        })
        .collect()
}
