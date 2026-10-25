//! Which raw TDLib requests `td_call` passes through, and on what terms.
//!
//! Every method falls in one class. Quill's own plumbing (its options, the
//! sign-in flow, closing the client) never runs for an agent. Requests that
//! touch the account itself or can spend Stars, TON or money run only after
//! the user clicks Allow in Quill, once per call. Everything else runs under
//! the chat grants: reads need `read` on every chat they name, writes need
//! `read_write` on the chat they act in, and writes that name no chat (the
//! profile, new groups, joining by link) ask the user like the deny list.

use crate::agent::grants::Scope;
use crate::agent::prompts::Risk;

/// What `td_call` does with one method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Never for agents; the text says why.
    Blocked(&'static str),
    /// Runs once the user allows it in Quill.
    Ask(Risk),
    /// Runs under the chat grants at this scope.
    Granted(Scope),
    /// Sends messages: `read_write`, the paid-message check and the pacing
    /// of every agent send.
    Send,
}

/// Quill's own plumbing.
const BLOCKED: &[(&str, &str)] = &[
    ("setTdlibParameters", "Quill sets up TDLib itself."),
    ("close", "Only Quill closes its Telegram connection."),
    ("destroy", "Only Quill closes its Telegram connection."),
    (
        "setOption",
        "Quill's options belong to Quill: agents never change online status, read receipts or typing.",
    ),
    ("setNetworkType", "Quill tracks the network itself."),
    ("setLogStream", "Quill owns its logs."),
    ("setLogVerbosityLevel", "Quill owns its logs."),
    ("setLogTagVerbosityLevel", "Quill owns its logs."),
    ("addLogMessage", "Quill owns its logs."),
    ("registerDevice", "Quill registers for notifications itself."),
    ("processPushNotification", "Quill handles notifications itself."),
    ("setAlarm", "Not available to agents."),
    ("sendCustomRequest", "Not available to agents."),
    ("answerCustomQuery", "Not available to agents."),
];

/// The sign-in flow: Quill's own screens drive it.
const SIGN_IN_PREFIXES: &[&str] = &[
    "setAuthentication",
    "checkAuthentication",
    "requestAuthentication",
    "requestQrCodeAuthentication",
    "resendAuthentication",
    "registerUser",
    "confirmQrCodeAuthentication",
];

/// The account itself: sign-out, sessions, password, phone, email.
const ACCOUNT: &[&str] = &[
    "deleteAccount",
    "logOut",
    "terminateSession",
    "terminateAllOtherSessions",
    "setPassword",
    "resetPassword",
    "cancelPasswordReset",
    "recoverPassword",
    "requestPasswordRecovery",
    "setRecoveryEmailAddress",
    "setLoginEmailAddress",
    "changePhoneNumber",
    "sendPhoneNumberCode",
    "checkPhoneNumberCode",
    "setAccountTtl",
    "disconnectWebsite",
    "disconnectAllWebsites",
    "transferChatOwnership",
    "toggleSessionCanAcceptCalls",
    "toggleSessionCanAcceptSecretChats",
    "setInactiveSessionTtl",
    "addProxy",
    "editProxy",
    "enableProxy",
    "disableProxy",
    "removeProxy",
    "confirmSession",
    "setUserPrivacySettingRules",
    "setReadDatePrivacySettings",
    "setNewChatPrivacySettings",
    "deleteChat",
];

/// Words in a method name that mean it can spend Stars, TON or money.
const MONEY_WORDS: &[&str] = &[
    "Star", "Ton", "Payment", "Gift", "Paid", "Invoice", "Premium", "Giveaway", "Purchase",
    "Withdraw", "Affiliate", "Subscription", "Offer", "Auction", "Stake",
];

/// Prefixes of methods that only read.
const READ_PREFIXES: &[&str] = &["get", "search", "check", "can", "parse", "validate", "is"];

/// Writes that name no chat but only open or fetch something.
const HARMLESS: &[&str] = &[
    "createPrivateChat",
    "createSupergroupChat",
    "createBasicGroupChat",
    "openChat",
    "closeChat",
    "loadChats",
    "loadSavedMessagesTopics",
    "downloadFile",
    "cancelDownloadFile",
    "readFilePart",
    "addFileToDownloads",
];

/// Methods that put messages in a chat.
const SENDS: &[&str] = &[
    "sendMessage",
    "sendMessageAlbum",
    "forwardMessages",
    "resendMessages",
    "sendInlineQueryResultMessage",
    "sendBotStartMessage",
    "sendQuickReplyShortcutMessages",
    "addLocalMessage",
];

fn is_read(method: &str) -> bool {
    READ_PREFIXES.iter().any(|prefix| {
        method
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
    })
}

/// A word counts only at a camel-case boundary: `Star` in `sendStarGift`,
/// not in `startGroupCall`.
fn has_word(method: &str, word: &str) -> bool {
    method.match_indices(word).any(|(at, _)| {
        let end = at + word.len();
        at > 0
            && method
                .get(end..)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(|c: char| !c.is_ascii_lowercase()))
    })
}

/// Whether a method or type name carries a money word.
pub fn names_money(name: &str) -> bool {
    MONEY_WORDS.iter().any(|word| has_word(name, word))
}

/// The class of `method`. `names_chat` says whether the request names a
/// chat (or a user, group or channel that maps to one).
pub fn classify(method: &str, names_chat: bool) -> Class {
    if let Some((_, why)) = BLOCKED.iter().find(|(name, _)| *name == method) {
        return Class::Blocked(why);
    }
    if SIGN_IN_PREFIXES.iter().any(|p| method.starts_with(p)) {
        return Class::Blocked("Quill's own screens handle signing in.");
    }
    if ACCOUNT.contains(&method) {
        return Class::Ask(Risk::Account);
    }
    if is_read(method) {
        return Class::Granted(Scope::Read);
    }
    if names_money(method) {
        return Class::Ask(Risk::Money);
    }
    if SENDS.contains(&method) {
        return Class::Send;
    }
    if HARMLESS.contains(&method) {
        return Class::Granted(Scope::Read);
    }
    if names_chat {
        Class::Granted(Scope::ReadWrite)
    } else {
        Class::Ask(Risk::Account)
    }
}

#[cfg(test)]
mod tests {
    use super::{Class, classify};
    use crate::agent::grants::Scope;
    use crate::agent::prompts::Risk;

    #[test]
    fn plumbing_and_sign_in_never_run() {
        assert!(matches!(classify("setOption", false), Class::Blocked(_)));
        assert!(matches!(classify("close", false), Class::Blocked(_)));
        assert!(matches!(
            classify("checkAuthenticationCode", false),
            Class::Blocked(_)
        ));
    }

    #[test]
    fn the_deny_list_asks_the_user() {
        for method in [
            "deleteAccount",
            "logOut",
            "terminateSession",
            "terminateAllOtherSessions",
            "setPassword",
            "changePhoneNumber",
        ] {
            assert_eq!(classify(method, false), Class::Ask(Risk::Account), "{method}");
        }
        for method in [
            "sendGift",
            "sendPaymentForm",
            "addPendingPaidMessageReaction",
            "upgradeGift",
            "transferGift",
            "launchPrepaidGiveaway",
            "editStarSubscription",
            "sendResoldGift",
        ] {
            assert_eq!(classify(method, true), Class::Ask(Risk::Money), "{method}");
        }
    }

    #[test]
    fn reads_and_writes_follow_the_grants() {
        assert_eq!(classify("getChatHistory", true), Class::Granted(Scope::Read));
        assert_eq!(classify("getStarTransactions", false), Class::Granted(Scope::Read));
        assert_eq!(classify("startGroupCall", true), Class::Granted(Scope::ReadWrite));
        assert_eq!(classify("toggleChatIsPinned", true), Class::Granted(Scope::ReadWrite));
        assert_eq!(classify("sendMessage", true), Class::Send);
        assert_eq!(classify("createPrivateChat", false), Class::Granted(Scope::Read));
        assert_eq!(classify("setBio", false), Class::Ask(Risk::Account));
        assert_eq!(classify("issue", false), Class::Ask(Risk::Account));
    }
}
