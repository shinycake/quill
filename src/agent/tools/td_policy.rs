//! Which raw TDLib requests `td_call` passes through, and on what terms.
//!
//! Every method falls in one class. Quill's own plumbing, raw file access
//! and the TON wallet never run for an agent. Requests that touch the
//! account or can spend Stars, TON or money run only after the user clicks
//! Allow in Quill, once per call. Reads run under the chat grants. A change
//! runs under the grants only when the schema says it acts on one chat (a
//! top-level `chat_id`); every other change asks the user.

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

/// Quill's own plumbing, and reads that would show what agents may not see.
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
    (
        "registerDevice",
        "Quill registers for notifications itself.",
    ),
    (
        "processPushNotification",
        "Quill handles notifications itself.",
    ),
    ("setAlarm", "Not available to agents."),
    ("sendCustomRequest", "Not available to agents."),
    ("answerCustomQuery", "Not available to agents."),
    (
        "addLocalMessage",
        "Agents cannot put messages in Quill that nobody sent.",
    ),
    (
        "getCurrentState",
        "It covers every chat; use the curated tools or reads of one chat.",
    ),
    (
        "getChatFolderChatCount",
        "It counts chats agents may not see.",
    ),
    (
        "searchFileDownloads",
        "Use download_file, which checks the chat a file came from.",
    ),
    (
        "getRemoteFile",
        "Use download_file, which checks the chat a file came from.",
    ),
    (
        "setDatabaseEncryptionKey",
        "Quill would lose its own database.",
    ),
    ("finishFileGeneration", "Quill generates its own files."),
    ("writeGeneratedFilePart", "Quill generates its own files."),
    (
        "setFileGenerationProgress",
        "Quill generates its own files.",
    ),
    (
        "acceptTermsOfService",
        "The user accepts terms of service in Quill.",
    ),
    (
        "getLanguagePackString",
        "It opens a database at any path; Quill handles languages.",
    ),
    ("openChat", "Quill tracks which chats are open."),
    ("closeChat", "Quill tracks which chats are open."),
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

/// Words that mark the TON wallet, which Quill never handles.
const TON_WALLET: &[&str] = &[
    "TonWallet",
    "TonConnect",
    "TonCenter",
    "WalletBot",
    "OnRamp",
];

/// The account itself, and reads that act for the user: sign-out, sessions,
/// password, phone, email, privacy, web logins, bot buttons, passport data.
/// Checked before the read prefixes, so `get…` names here still ask.
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
    "checkPasswordRecoveryCode",
    "setRecoveryEmailAddress",
    "checkRecoveryEmailAddressCode",
    "setLoginEmailAddress",
    "checkLoginEmailAddressCode",
    "changePhoneNumber",
    "sendPhoneNumberCode",
    "checkPhoneNumberCode",
    "sharePhoneNumber",
    "addContact",
    "importContacts",
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
    "getCallbackQueryAnswer",
    "getLoginUrl",
    "getExternalLink",
    "getEmojiSuggestionsUrl",
    "getWebAppUrl",
    "getWebAppLinkUrl",
    "getMainWebApp",
    "openWebApp",
    "validateOrderInfo",
    "getPassportElement",
    "getAllPassportElements",
    "getPassportAuthorizationForm",
    "getPassportAuthorizationFormAvailableElements",
    "getInlineQueryResults",
    "getGuardBotWebAppUrl",
    "getUserLink",
    "setPersonalChat",
    "getVideoChatRtmpUrl",
    "replaceVideoChatRtmpUrl",
    "getLiveStoryRtmpUrl",
    "replaceLiveStoryRtmpUrl",
    "getSavedOrderInfo",
    "deleteSavedOrderInfo",
    "getProxies",
    "getActiveSessions",
    "getConnectedWebsites",
    "getPasswordState",
    "getRecoveryEmailAddress",
    "setProfileShareTheme",
];

/// Methods that move Stars or boosts without a money word in their name.
const MONEY: &[&str] = &[
    "boostChat",
    "allowUnpaidMessagesFromUser",
    "toggleDirectMessagesChatTopicCanSendUnpaidMessages",
    "joinChatByInviteLink",
    "getChatRevenueWithdrawalUrl",
    "getStarWithdrawalUrl",
    "getGramWithdrawalUrl",
    "getStarAdAccountUrl",
    "getUpgradedGiftWithdrawalUrl",
];

/// Words in a change's name that mean it posts on the user's profile or
/// streams as them: stories post as the user from Saved Messages.
const PROFILE_WORDS: &[&str] = &["Story", "Stories", "StoryAlbum", "LiveStory"];

/// Words in a method or type name that mean it can spend Stars, TON or
/// money. Plurals are listed: a word counts only at a camel-case boundary.
const MONEY_WORDS: &[&str] = &[
    "Star",
    "Stars",
    "Ton",
    "Gram",
    "Grams",
    "Payment",
    "Payments",
    "Gift",
    "Gifts",
    "Paid",
    "Invoice",
    "Invoices",
    "Premium",
    "Giveaway",
    "Giveaways",
    "Purchase",
    "Purchases",
    "Withdraw",
    "Withdrawal",
    "Affiliate",
    "Subscription",
    "Subscriptions",
    "Offer",
    "Offers",
    "Auction",
    "Stake",
    "Revenue",
];

/// Prefixes of methods that only read.
const READ_PREFIXES: &[&str] = &["get", "search", "check", "can", "parse", "is"];

/// Changes that name no chat but only open something; they run like reads.
const OPENS: &[&str] = &[
    "createPrivateChat",
    "createSupergroupChat",
    "createBasicGroupChat",
    "loadChats",
    "loadSavedMessagesTopics",
];

/// Methods that put messages in a chat.
pub const SENDS: &[&str] = &[
    "sendMessage",
    "sendMessageAlbum",
    "forwardMessages",
    "resendMessages",
    "sendInlineQueryResultMessage",
    "sendBotStartMessage",
    "sendQuickReplyShortcutMessages",
];

pub fn is_read(method: &str) -> bool {
    READ_PREFIXES.iter().any(|prefix| {
        method
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
    })
}

/// A word counts only at a camel-case boundary: `Star` in `sendStarGift`,
/// not in `startGroupCall` or `getStartedBots`.
fn has_word(name: &str, word: &str) -> bool {
    name.match_indices(word).any(|(at, _)| {
        let end = at + word.len();
        at > 0
            && name.get(end..).is_some_and(|rest| {
                rest.is_empty() || rest.starts_with(|c: char| !c.is_ascii_lowercase())
            })
    })
}

/// Whether a method or type name carries a money word.
pub fn names_money(name: &str) -> bool {
    MONEY_WORDS.iter().any(|word| has_word(name, word))
}

/// What the schema says about a method's top-level fields.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shape {
    /// A `file_id`: raw file access.
    pub takes_file: bool,
    /// A `chat_id: int53`: the chat it acts in.
    pub chat_scoped: bool,
    /// A `group_call_id`: small local ids that reach any chat's call.
    pub group_call: bool,
}

/// The class of `method` with the given top-level fields.
pub fn classify(method: &str, shape: Shape) -> Class {
    if let Some((_, why)) = BLOCKED.iter().find(|(name, _)| *name == method) {
        return Class::Blocked(why);
    }
    if SIGN_IN_PREFIXES.iter().any(|p| method.starts_with(p)) {
        return Class::Blocked("Quill's own screens handle signing in.");
    }
    if TON_WALLET.iter().any(|word| method.contains(word)) {
        return Class::Blocked("Quill never handles the TON wallet; use Telegram's official apps.");
    }
    if shape.takes_file {
        return Class::Blocked("Use download_file, which checks the chat a file came from.");
    }
    if ACCOUNT.contains(&method) {
        return Class::Ask(Risk::Account);
    }
    if MONEY.contains(&method) {
        return Class::Ask(Risk::Money);
    }
    if shape.group_call && !shape.chat_scoped {
        return Class::Ask(Risk::Account);
    }
    if is_read(method) {
        return Class::Granted(Scope::Read);
    }
    if names_money(method) {
        return Class::Ask(Risk::Money);
    }
    if PROFILE_WORDS.iter().any(|word| has_word(method, word)) {
        return Class::Ask(Risk::Account);
    }
    if SENDS.contains(&method) {
        return Class::Send;
    }
    if OPENS.contains(&method) {
        return Class::Granted(Scope::Read);
    }
    if shape.chat_scoped {
        Class::Granted(Scope::ReadWrite)
    } else {
        Class::Ask(Risk::Account)
    }
}

#[cfg(test)]
mod tests {
    use super::{Class, Shape, names_money};
    use crate::agent::grants::Scope;
    use crate::agent::prompts::Risk;

    fn classify(method: &str, takes_file: bool, chat_scoped: bool) -> Class {
        super::classify(
            method,
            Shape {
                takes_file,
                chat_scoped,
                group_call: false,
            },
        )
    }

    #[test]
    fn plumbing_files_and_the_wallet_never_run() {
        assert!(matches!(
            classify("setOption", false, false),
            Class::Blocked(_)
        ));
        assert!(matches!(
            classify("checkAuthenticationCode", false, false),
            Class::Blocked(_)
        ));
        assert!(matches!(
            classify("readFilePart", true, false),
            Class::Blocked(_)
        ));
        assert!(matches!(
            classify("getTonWalletSecretPhrase", false, false),
            Class::Blocked(_)
        ));
        assert!(matches!(
            classify("addLocalMessage", false, true),
            Class::Blocked(_)
        ));
        assert!(matches!(
            classify("getCurrentState", false, false),
            Class::Blocked(_)
        ));
        for method in [
            "sendTonCenterApiRequest",
            "checkWalletBotBalance",
            "createOnRampPaymentSession",
            "setDatabaseEncryptionKey",
        ] {
            assert!(
                matches!(classify(method, false, false), Class::Blocked(_)),
                "{method}"
            );
        }
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
            "getCallbackQueryAnswer",
            "getWebAppUrl",
            "sharePhoneNumber",
            "postStory",
            "startLiveStory",
            "setChatPinnedStories",
            "deleteStoryAlbum",
            "getLiveStoryRtmpUrl",
            "getUserLink",
            "getActiveSessions",
            "getProxies",
        ] {
            assert_eq!(
                classify(method, false, true),
                Class::Ask(Risk::Account),
                "{method}"
            );
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
            "joinChatByInviteLink",
            "boostChat",
            "getChatRevenueWithdrawalUrl",
            "getGramWithdrawalUrl",
            "getUpgradedGiftWithdrawalUrl",
        ] {
            assert_eq!(
                classify(method, false, true),
                Class::Ask(Risk::Money),
                "{method}"
            );
        }
    }

    #[test]
    fn only_chat_scoped_changes_follow_the_grants() {
        assert_eq!(
            classify("getChatHistory", false, true),
            Class::Granted(Scope::Read)
        );
        assert_eq!(
            classify("startGroupCall", false, true),
            Class::Granted(Scope::ReadWrite)
        );
        assert_eq!(classify("sendMessage", false, true), Class::Send);
        assert_eq!(
            classify("createPrivateChat", false, false),
            Class::Granted(Scope::Read)
        );
        assert_eq!(classify("setBio", false, false), Class::Ask(Risk::Account));
        assert_eq!(
            super::classify(
                "getGroupCall",
                Shape {
                    group_call: true,
                    ..Shape::default()
                }
            ),
            Class::Ask(Risk::Account)
        );
        assert_eq!(
            classify("getStory", false, true),
            Class::Granted(Scope::Read)
        );
        assert_eq!(
            classify("editChatFolder", false, false),
            Class::Ask(Risk::Account)
        );
    }

    #[test]
    fn money_words_match_whole_camel_case_words() {
        assert!(names_money("reactionTypePaid"));
        assert!(names_money("suggestedPostPriceGram"));
        assert!(names_money("getStarsTransactions"));
        assert!(!names_money("startGroupCall"));
        assert!(!names_money("getStartedBots"));
        assert!(!names_money("inputMessageText"));
    }
}
