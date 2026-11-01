//! Notifications and unread counts from every signed-in account
//! (`parity:notify-all-accounts`).
//!
//! Telegram Desktop keeps the session of every account connected while the
//! app runs. Accounts that are not on screen still receive updates, raise
//! notifications (unless Settings > Notifications > "Show notifications
//! from: All accounts" is off) and add their unread count to the app badge
//! and to their row in the account list. Quill runs one TDLib client per
//! background account for the same reason; this module holds the pure
//! decisions around them, the UI (`ui::background_accounts`) runs the
//! clients.
#![deny(clippy::indexing_slicing)]

use crate::ids::{AccountKey, ChatId};
use crate::notify::{NotificationAction, QueuedNotification};
use crate::settings::BadgePrefs;
use crate::state::Session;
use crate::telegram::envelope::AuthorizationState;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// tdesktop `System::computeSkipState`: with "All accounts" off, only the
/// account on screen notifies.
pub fn account_may_notify(notify_from_all: bool, is_active: bool) -> bool {
    is_active || notify_from_all
}

/// tdesktop `BuildMultiAccountSection`: the "Show notifications from"
/// section exists only with two or more accounts.
pub fn notify_from_all_setting_visible(account_count: usize) -> bool {
    account_count >= 2
}

/// tdesktop `MainWindow::updateTitle`: the window title names the signed-in
/// user only while more than one account is signed in
/// (`accountsAuthedCount() > 1`).
pub fn account_name_in_title(signed_in_accounts: usize) -> bool {
    signed_in_accounts > 1
}

/// What a background account's client is doing, for its row in the
/// account list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundStatus {
    /// Restoring the session; no updates yet.
    Connecting,
    /// Signed in and receiving updates.
    Ready,
    /// The account has no session: it needs the sign-in flow, which only
    /// runs for the account on screen.
    NeedsSignIn,
    /// The session ended (logged out here or from another device).
    SignedOut,
    /// The client could not start (the message is the blocker's
    /// user-facing text, never a secret).
    Failed(String),
}

impl BackgroundStatus {
    /// Short label for the account list; `None` while nothing needs saying.
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Connecting | Self::Ready => None,
            Self::NeedsSignIn => Some("Not signed in"),
            Self::SignedOut => Some("Signed out"),
            Self::Failed(message) => Some(message),
        }
    }

    /// The client cannot do anything more in the background: close it and
    /// keep only the status until the user switches to the account.
    pub fn stops_client(&self) -> bool {
        matches!(self, Self::NeedsSignIn | Self::SignedOut | Self::Failed(_))
    }
}

/// Classify a background client's authorization state.
pub fn background_status(auth: &AuthorizationState) -> BackgroundStatus {
    match auth {
        AuthorizationState::Ready => BackgroundStatus::Ready,
        AuthorizationState::WaitTdlibParameters | AuthorizationState::Unknown(_) => {
            BackgroundStatus::Connecting
        }
        AuthorizationState::WaitPhoneNumber
        | AuthorizationState::WaitPremiumPurchase { .. }
        | AuthorizationState::WaitEmailAddress
        | AuthorizationState::WaitEmailCode { .. }
        | AuthorizationState::WaitCode { .. }
        | AuthorizationState::WaitOtherDeviceConfirmation { .. }
        | AuthorizationState::WaitRegistration { .. }
        | AuthorizationState::WaitPassword { .. } => BackgroundStatus::NeedsSignIn,
        AuthorizationState::LoggingOut
        | AuthorizationState::Closing
        | AuthorizationState::Closed => BackgroundStatus::SignedOut,
    }
}

/// One account's unread badge: tdesktop `unreadWithMentionsBadge` and
/// `unreadWithMentionsBadgeMuted`, computed with the account's own badge
/// counter settings (Quill's `tray::badge_count`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccountUnread {
    pub count: u32,
    /// Every counted unread chat is muted (the badge is drawn grey).
    pub muted: bool,
}

impl AccountUnread {
    pub fn of(session: &Session) -> Self {
        let prefs = session.settings.badge_prefs;
        let count = crate::tray::badge_count(session, &prefs);
        let unmuted = if prefs.include_muted {
            crate::tray::badge_count(
                session,
                &BadgePrefs {
                    include_muted: false,
                    ..prefs
                },
            )
        } else {
            count
        };
        Self {
            count,
            muted: count > 0 && unmuted == 0,
        }
    }

    /// tdesktop `Domain::updateUnreadBadge` / `OtherAccountsUnreadState`:
    /// counts add up, and the sum is muted only when every account that has
    /// unread messages is muted.
    pub fn combine(self, other: Self) -> Self {
        let count = self.count.saturating_add(other.count);
        let muted =
            count > 0 && (self.count == 0 || self.muted) && (other.count == 0 || other.muted);
        Self { count, muted }
    }

    fn pack(self) -> u64 {
        (u64::from(self.count) << 1) | u64::from(self.muted)
    }

    fn unpack(bits: u64) -> Self {
        Self {
            count: (bits >> 1) as u32,
            muted: bits & 1 == 1,
        }
    }
}

/// The background accounts' combined unread, set by the UI after each
/// background poll. The tray icon, the app icon badge and the window title
/// add it to the account on screen (tdesktop `Domain::unreadBadge` sums
/// every account).
static OTHER_ACCOUNTS_UNREAD: AtomicU64 = AtomicU64::new(0);

pub fn set_other_accounts_unread(unread: AccountUnread) {
    OTHER_ACCOUNTS_UNREAD.store(unread.pack(), Ordering::Relaxed);
}

pub fn other_accounts_unread() -> AccountUnread {
    AccountUnread::unpack(OTHER_ACCOUNTS_UNREAD.load(Ordering::Relaxed))
}

/// The app-wide badge: the account on screen plus every background one.
pub fn total_unread(active: Option<&Session>) -> AccountUnread {
    active
        .map(|session| ACTIVE_UNREAD.with_borrow_mut(|memo| memo.get(session)))
        .unwrap_or_default()
        .combine(other_accounts_unread())
}

thread_local! {
    /// The tray and the icon badge both ask on every wake of their loop.
    static ACTIVE_UNREAD: std::cell::RefCell<UnreadMemo> = Default::default();
}

/// Memo for [`AccountUnread::of`]: the count walks every chat and forum
/// topic, and only changes when the session applies an update
/// (`Session::revision`), the badge preferences change, or another session
/// is asked about.
#[derive(Debug, Default)]
pub struct UnreadMemo {
    key: Option<(usize, u64, usize, BadgePrefs)>,
    unread: AccountUnread,
}

impl UnreadMemo {
    pub fn get(&mut self, session: &Session) -> AccountUnread {
        let key = (
            std::ptr::from_ref(session) as usize,
            session.revision,
            session.chats.len(),
            session.settings.badge_prefs,
        );
        if self.key.as_ref() != Some(&key) {
            self.unread = AccountUnread::of(session);
            self.key = Some(key);
        }
        self.unread
    }
}

/// What one poll of a background account hands the UI.
#[derive(Debug, Default)]
pub struct BackgroundDrain {
    /// Notifications to show (empty when "All accounts" is off).
    pub notifications: Vec<QueuedNotification>,
    /// Chats read elsewhere: withdraw their shown notifications.
    pub clears: Vec<ChatId>,
    /// Custom notification sounds whose download just finished.
    pub sound_plays: Vec<PathBuf>,
    /// Flash the taskbar / bounce the Dock icon for a new message.
    pub attention: bool,
}

/// Take what a background account's session queued for the screen. The
/// session keeps no chat open and no window shows it, so its notification
/// decisions are the ones tdesktop makes for an account that is not on
/// screen. With "All accounts" off nothing is shown (tdesktop skips the
/// notification), but the queues are still emptied so nothing piles up
/// or pops out after a switch. A force-reply request never applies to an
/// account that is not on screen.
pub fn drain_background(session: &mut Session, notify_from_all: bool) -> BackgroundDrain {
    let settings = &mut session.settings;
    let notifications = std::mem::take(&mut settings.pending_notifications);
    let sound_plays = std::mem::take(&mut settings.pending_sound_plays);
    let attention = std::mem::take(&mut settings.pending_attention);
    let clears = std::mem::take(&mut settings.pending_notification_clears);
    session.bots.pending_force_reply = None;
    if !account_may_notify(notify_from_all, false) {
        return BackgroundDrain {
            clears,
            ..BackgroundDrain::default()
        };
    }
    BackgroundDrain {
        notifications,
        clears,
        sound_plays,
        attention,
    }
}

/// The incoming call still ringing in `session`: `(call id, caller user id,
/// video)`.
pub fn ringing_incoming_call(session: &Session) -> Option<(i32, i64, bool)> {
    let call = session.calls.active_call.as_ref()?;
    (!call.is_outgoing
        && matches!(
            call.state,
            crate::telegram::envelope::CallState::Pending { .. }
        ))
    .then_some((call.id, call.user_id, call.is_video))
}

/// What to do about an incoming call on an account that is not on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundCallPlan {
    /// Ring: the call prompt, the ringtone and the OS notification.
    Ring,
    /// tdesktop `Calls::Instance` handles one call at a time across every
    /// account: a call that comes in while another is in progress or
    /// ringing is declined as busy.
    DeclineBusy,
}

pub fn plan_background_call(on_screen_in_call: bool, another_ringing: bool) -> BackgroundCallPlan {
    if on_screen_in_call || another_ringing {
        BackgroundCallPlan::DeclineBusy
    } else {
        BackgroundCallPlan::Ring
    }
}

/// What a click on a notification does, by the account it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickPlan {
    /// The account on screen: the usual handling.
    Active,
    /// "Mark as read" on another account: read it there, stay put.
    MarkReadInBackground(AccountKey),
    /// Open or Reply on another account: switch to it, then open the chat
    /// (tdesktop `Manager::openNotificationMessage` activates the account
    /// of the notification first).
    SwitchThenOpen(AccountKey),
    /// An account Quill no longer knows (removed since): nothing.
    Ignore,
}

/// Plan a notification click. `tag_account` is the account part of the
/// notification tag (`account:<key>`, see `notify::parse_notification_tag`).
pub fn plan_click(
    tag_account: &str,
    action: NotificationAction,
    active: &AccountKey,
    known: &[AccountKey],
) -> ClickPlan {
    let Some(key) = tag_account
        .strip_prefix("account:")
        .and_then(AccountKey::new)
    else {
        return ClickPlan::Ignore;
    };
    if &key == active {
        return ClickPlan::Active;
    }
    if !known.contains(&key) {
        return ClickPlan::Ignore;
    }
    match action {
        NotificationAction::MarkRead => ClickPlan::MarkReadInBackground(key),
        NotificationAction::Open | NotificationAction::Reply => ClickPlan::SwitchThenOpen(key),
    }
}

#[cfg(test)]
mod tests;
