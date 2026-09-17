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

/// SHA-256 of `schema/td_api.tl` fetched from that commit.
pub const TD_API_TL_SHA256: &str =
    "4a637fab0cd33e5be6460128865bb403d259ff3a054c14a55d27e6b04f008cb5";

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
