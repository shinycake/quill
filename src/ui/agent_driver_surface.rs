//! What the window shows, as the UI driver sees it: one pure function of
//! `QuillApp`'s state (`driver_surface`), computed again for every command
//! and every step of one. Nothing here reads the element tree, so a label
//! in the window can never widen what an agent may see or do.
//!
//! The driver follows the chat grants. With "All other chats" off it sees
//! the chat list's granted rows and the open granted chat, nothing else.
//! With it on ([`DriverScope::Everything`]) most of the window is allowed;
//! the hard list stays blocked in every mode: the agent's own prompts, the
//! Agent access settings, log out and account deletion, payments, Stars,
//! gifts and Premium, the passcode lock and the passcode and 2FA settings,
//! Active Sessions, the file-open warning and the chat and account exports.
#![deny(clippy::indexing_slicing)]

use super::agent_state::{active_account, is_secret_chat_id};
use super::app::{PaneMode, QuillApp};
use super::esc_stack::ESC_LAYERS;
use super::first_run_state::FirstRun;
use super::shell::QuillShell;
use quill::agent::grants::{self, ChatClass, Scope};
use quill::agent::ui_driver::surface::{DriverScope, Surface};
use quill::ids::ChatId;
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind};

/// Kit dialogs agents never see or use, in any mode (`register_dialogs!`
/// names). A test checks every name is registered.
pub(super) const HARD_DIALOGS: &[&str] = &[
    "AgentRequest",
    "AgentConsent",
    "LogOut",
    "AccountLifecycle",
    "Passcode",
    "TwoFa",
    "Passkeys",
    "PasskeyCeremony",
    "PasskeyDelete",
    "PaymentForm",
    "PaymentReceipt",
    "Stars",
    "StarsGift",
    "StarsRating",
    "SendGift",
    "GiftTransfer",
    "GiftMarket",
    "GiftValue",
    "GiftCodeLink",
    "PremiumFeatures",
    "PremiumLimit",
    "Subscriptions",
    "Money",
    "Grams",
    "GramsWithdraw",
    "EarnWithdraw",
    "EarnSetupConfirm",
    "EarnTonTx",
    "EarnStarTx",
    "PaidSendConfirm",
    "PaidReaction",
    "PaidPrice",
    "RemoveFee",
    "Websites",
    "CallbackPassword",
    "LoginUrlConfirm",
    "WebAppConfirm",
    "Accounts",
    "Marketplace",
    "Giveaway",
    "SuggestPrice",
    "SuggestGramsNeeded",
    "LiveStoryStars",
    "DebugModeConfirm",
    // Local effects outside TDLib: opening a file with the OS, bulk export.
    "FileOpenConfirm",
    "ChatExport",
    "AccountExport",
];

/// Hand-drawn layers (`esc_stack::ESC_LAYERS` names) blocked in any mode.
pub(super) const HARD_LAYERS: &[&str] = &[
    "first-run-intro",
    "payment-dialog",
    "payment-receipt",
    "sessions",
    "privacy",
    "earn-layer",
    "screen-privacy",
    "pending-delete",
    "pending-stop-poll",
];

/// Layers the granted scope allows: what reading and writing in the chat
/// list and the open chat takes. Everything else blocks it.
pub(super) const GRANTED_LAYERS: &[&str] = &[
    "context-menus",
    "link-popup",
    "composer-code-language",
    "composer-date-dialog",
    "composer-link-dialog",
    "gif-panel",
    "sticker-panel",
    "mute-menu",
    "ttl-picker",
    "send-as",
    "reply-panels",
    "forward-result",
    "chat-search",
    "search",
    "selection-mode",
    "pending-reply",
    "pending-edit",
];

/// The user-facing name of a layer, for the blocked note.
fn layer_name(layer: &str) -> String {
    match layer {
        "sessions" => "the active sessions list".to_string(),
        "privacy" => "the Privacy settings".to_string(),
        "payment-dialog" | "payment-receipt" => "a payment".to_string(),
        "earn-layer" => "a revenue page".to_string(),
        "media-viewer" => "the media viewer".to_string(),
        "info-panel" | "shared-media" => "a chat's info".to_string(),
        "forward-picker" => "the forward picker".to_string(),
        "first-run-intro" => "the welcome screen".to_string(),
        other => format!("an overlay ({})", other.replace('-', " ")),
    }
}

impl QuillApp {
    /// How far the driver reaches: everything once "All other chats" is on.
    pub(super) fn driver_scope(&self) -> DriverScope {
        if grants::for_account(&active_account()).all_chats.is_some() {
            DriverScope::Everything
        } else {
            DriverScope::Granted
        }
    }

    /// The grant `chat_id` has, by the same rule the chat tools use.
    pub(super) fn driver_chat_scope(&self, chat_id: i64) -> Option<Scope> {
        grants::for_account(&active_account()).scope_for(chat_id, self.driver_chat_class(chat_id))
    }

    /// The class of `chat_id` its grant defaults follow.
    pub(super) fn driver_chat_class(&self, chat_id: i64) -> ChatClass {
        let session = self.session();
        match session.and_then(|s| s.chats.get(&chat_id)) {
            Some(chat) => match chat.kind {
                ChatKind::Private { user_id } => {
                    if session.is_some_and(|s| s.is_saved_messages(ChatId(chat_id))) {
                        ChatClass::Saved
                    } else if session.is_some_and(|s| s.is_bot_user(user_id.0)) {
                        ChatClass::Bot
                    } else {
                        ChatClass::Other
                    }
                }
                ChatKind::Secret { .. } => ChatClass::Secret,
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel,
                } => {
                    let creator = session.is_some_and(|s| {
                        s.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    });
                    if is_channel && creator {
                        ChatClass::OwnChannel
                    } else {
                        ChatClass::Other
                    }
                }
                ChatKind::BasicGroup { .. } | ChatKind::Unknown => ChatClass::Other,
            },
            None if is_secret_chat_id(chat_id) => ChatClass::Secret,
            None => ChatClass::Other,
        }
    }

    /// Whether the driver may open `chat_id` through the window.
    pub(super) fn driver_may_open_chat(&self, chat_id: i64) -> bool {
        self.driver_chat_scope(chat_id).is_some()
    }

    /// The surface the window shows now.
    pub(super) fn driver_surface(&self) -> Surface {
        let blocked = |what: &str| Surface::Blocked(what.to_string());
        if self.pane_mode() != PaneMode::Ready {
            return blocked("the sign-in screen");
        }
        if self.account.passcode.locked {
            return blocked("the passcode lock screen");
        }
        if !matches!(self.first_run, FirstRun::Done) {
            return blocked("the welcome screen");
        }
        if self.settings.open {
            return blocked("Settings");
        }
        if self.agent_ui.consent.is_some() || self.agent_request_open() {
            return blocked("a request to the user");
        }
        let scope = self.driver_scope();
        if let Some(dialog) = QuillShell::open_dialog_name(self) {
            if dialog == "AgentRequest" {
                return blocked("a request to the user");
            }
            if scope == DriverScope::Granted || HARD_DIALOGS.contains(&dialog) {
                return blocked(&format!("a dialog ({dialog})"));
            }
        }
        for layer in ESC_LAYERS.iter().filter(|layer| (layer.is_open)(self)) {
            if HARD_LAYERS.contains(&layer.name)
                || (scope == DriverScope::Granted && !GRANTED_LAYERS.contains(&layer.name))
            {
                return Surface::Blocked(layer_name(layer.name));
            }
        }
        match self.open_chat_id() {
            None => Surface::ChatList,
            Some(chat) => match self.driver_chat_scope(chat.0) {
                Some(scope) => Surface::Chat {
                    chat_id: chat.0,
                    scope,
                },
                None => blocked("an ungranted chat"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GRANTED_LAYERS, HARD_DIALOGS, HARD_LAYERS};
    use crate::ui::esc_stack::ESC_LAYERS;
    use crate::ui::shell::QuillShell;

    #[test]
    fn every_hard_dialog_is_registered() {
        let names = QuillShell::dialog_names();
        let missing: Vec<_> = HARD_DIALOGS
            .iter()
            .filter(|name| !names.contains(name))
            .collect();
        assert!(missing.is_empty(), "unknown dialogs: {missing:?}");
    }

    #[test]
    fn every_listed_layer_exists_and_no_layer_is_both() {
        let names: Vec<_> = ESC_LAYERS.iter().map(|layer| layer.name).collect();
        for layer in HARD_LAYERS.iter().chain(GRANTED_LAYERS) {
            assert!(names.contains(layer), "unknown layer {layer}");
        }
        for layer in HARD_LAYERS {
            assert!(
                !GRANTED_LAYERS.contains(layer),
                "{layer} is hard and allowed"
            );
        }
    }
}
