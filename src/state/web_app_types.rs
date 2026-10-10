//! Mini-app state the reducer keeps for the UI: one-shot answers the UI
//! takes on its next poll, plus the attachment menu bots.

use crate::telegram::envelope::{AttachmentMenuBot, FoundWebApp};
use crate::web_app::LaunchSource;
use std::collections::HashMap;

/// The open call in flight (`openWebApp`, `getWebAppUrl`, `getMainWebApp`,
/// `getWebAppLinkUrl`): what the answer is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAppPending {
    pub bot_user_id: i64,
    pub chat_id: Option<i64>,
    pub source: LaunchSource,
}

/// The answer to an open call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebAppOpenResult {
    Opened {
        bot_user_id: i64,
        chat_id: Option<i64>,
        source: LaunchSource,
        /// `webAppInfo.launch_id`; 0 for `getWebAppUrl` and link apps.
        launch_id: i64,
        url: String,
    },
    Failed {
        bot_user_id: i64,
        message: String,
    },
}

/// Where a bot stands on messaging the person (`canBotSendMessages`,
/// `allowBotToSendMessages`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAccessResult {
    /// `canBotSendMessages` said ok.
    Allowed,
    /// `canBotSendMessages` answered 404: ask the person.
    NeedsConsent,
    /// `allowBotToSendMessages` said ok.
    Granted,
    /// A request failed; treat as refused.
    Denied,
}

/// `sendWebAppCustomRequest` answers, keyed by the app's `req_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRequestReply {
    pub req_id: String,
    /// The JSON result, or an error word for the app.
    pub result: Result<String, String>,
}

/// Everything mini-app related on the session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebApps {
    /// The open call in flight.
    pub pending: Option<WebAppPending>,
    /// One-shot: the open call's answer.
    pub open_result: Option<WebAppOpenResult>,
    /// `updateAttachmentMenuBots`: the bots in the attachment menu.
    pub attachment_menu_bots: Vec<AttachmentMenuBot>,
    /// One-shot: `getAttachmentMenuBot` answer (for adding a bot), or a
    /// failure message.
    pub attachment_menu_bot: Option<Result<AttachmentMenuBot, String>>,
    /// One-shot: `toggleBotIsAddedToAttachmentMenu` outcome,
    /// `(bot_user_id, added)` or a failure message.
    pub attachment_menu_toggled: Option<Result<(i64, bool), String>>,
    /// One-shot: write-access answers, `(bot_user_id, result)`.
    pub write_access: Option<(i64, WriteAccessResult)>,
    /// One-shot: `searchWebApp` answer for a `t.me/bot/app` link,
    /// `(bot_user_id, app)` or a failure message.
    pub found: Option<Result<(i64, FoundWebApp), String>>,
    /// `req_id`s of custom requests in flight, by `RequestId`.
    pub custom_requests: HashMap<u64, String>,
    /// One-shot: custom request answers.
    pub custom_replies: Vec<CustomRequestReply>,
    /// Apps tab: `getGrossingWebAppBots` bot ids (`None` until loaded).
    pub grossing_bots: Option<Vec<i64>>,
    pub grossing_loading: bool,
    /// One-shot: `sendWebAppData` outcome (`Ok` or a failure message).
    pub data_sent: Option<Result<(), String>>,
}
