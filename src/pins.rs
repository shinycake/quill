//! Exact pins for this checkout. Recheck together; never refresh one layer alone.

/// crates.io `gpui-kit` version selected on 2026-09-17.
/// Research snapshot cited git `d604a2ace` as package 0.6.1; crates.io default
/// is now 0.6.1 (0.6.0 also exists). The whole UI family is this Kit release
/// (`gpui-pre` ^0.3.1, resolved in Cargo.lock).
pub const GPUI_KIT_VERSION: &str = "0.6.1";

/// Official TDLib git commit used as the schema/runtime baseline.
pub const TDLIB_GIT_COMMIT: &str = "d1085f9cebc5a62379991ae1652673954f229c1f";

/// CMake `project(TDLib VERSION …)` at [`TDLIB_GIT_COMMIT`].
pub const TDLIB_CMAKE_VERSION: &str = "1.8.67";

/// Official `td/generate/scheme/td_api.tl` at [`TDLIB_GIT_COMMIT`].
/// SHA-256 of the **upstream** file (not of a locally truncated copy).
pub const TD_API_TL_SHA256: &str =
    "326b65b41442901ad6bf0ca2f7c356ae54365d6c343956a62e06a8b3cb305e87";

/// Byte length of that official schema file.
pub const TD_API_TL_BYTES: usize = 1_152_505;

/// Raw GitHub URL for the official schema at [`TDLIB_GIT_COMMIT`].
pub const TD_API_TL_UPSTREAM_URL: &str = "https://raw.githubusercontent.com/tdlib/td/d1085f9cebc5a62379991ae1652673954f229c1f/td/generate/scheme/td_api.tl";

/// Relative path of the vendored schema.
pub const TD_API_TL_PATH: &str = "schema/td_api.tl";

/// Schema constructors this crate must keep typed coverage for.
pub const REQUIRED_SCHEMA_CONSTRUCTORS: &[&str] = &[
    "error",
    "ok",
    "authorizationStateWaitTdlibParameters",
    "authorizationStateWaitPhoneNumber",
    "authorizationStateWaitPremiumPurchase",
    "authorizationStateWaitEmailAddress",
    "authorizationStateWaitEmailCode",
    "authorizationStateWaitCode",
    "authorizationStateWaitOtherDeviceConfirmation",
    "authorizationStateWaitRegistration",
    "authorizationStateWaitPassword",
    "authorizationStateReady",
    "authorizationStateLoggingOut",
    "authorizationStateClosing",
    "authorizationStateClosed",
    "updateAuthorizationState",
    "updateNewMessage",
    "updateMessageSendAcknowledged",
    "updateMessageSendSucceeded",
    "updateMessageSendFailed",
    "updateDeleteMessages",
    "updateChatPosition",
    "updateConnectionState",
    "setTdlibParameters",
    "getAuthorizationState",
    "loadChats",
    "getChatHistory",
    "sendMessage",
    "logOut",
    "close",
    "chatListMain",
    "chatPosition",
    "chatTypePrivate",
    "chatTypeBasicGroup",
    "chatTypeSupergroup",
    "chatTypeSecret",
    "messageText",
    "inputMessageText",
    "formattedText",
];
