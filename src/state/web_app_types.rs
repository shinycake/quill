//! Mini-app state the reducer keeps for the UI: one-shot answers the UI
//! takes on its next poll, plus the attachment menu bots.

use crate::telegram::envelope::{AttachmentMenuBot, FoundWebApp};
use crate::web_app::LaunchSource;
use std::collections::HashMap;

/// The open call in flight (`openWebApp`, `getWebAppUrl`, `getMainWebApp`,
/// `getWebAppLinkUrl`): what the answer is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAppPending {
    /// The open call this is for: a second launch before the first
    /// answers keeps its own context.
    pub request: crate::ids::RequestId,
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

/// The outcome of one contact-card send for a mini app's phone request.
/// It names the `sendMessage` request it answers: only the flow that sent
/// that request takes it, so a late answer never finishes a later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhoneShare {
    Sent(crate::ids::RequestId),
    Refused(crate::ids::RequestId),
}

impl PhoneShare {
    pub fn request(self) -> crate::ids::RequestId {
        match self {
            Self::Sent(request) | Self::Refused(request) => request,
        }
    }

    pub fn is_sent(self) -> bool {
        matches!(self, Self::Sent(_))
    }
}

/// The outcome of one `toggleBotIsAddedToAttachmentMenu`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentToggle {
    pub bot_user_id: i64,
    /// Whether the bot is in the menu now, or why the change failed.
    pub result: Result<bool, String>,
}

/// `sendWebAppCustomRequest` answers, keyed by the app's `req_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRequestReply {
    pub req_id: String,
    /// The JSON result, or an error word for the app.
    pub result: Result<String, String>,
}

impl WebApps {
    /// Replace the unconsumed open answer. An `Opened` answer it pushes out
    /// was never shown, but TDLib started that app: its launch is queued
    /// for `closeWebApp`, so the bot keeps no live session.
    pub fn replace_open_result(&mut self, result: Option<WebAppOpenResult>) {
        if let Some(WebAppOpenResult::Opened { launch_id, .. }) =
            std::mem::replace(&mut self.open_result, result)
            && launch_id != 0
        {
            self.dropped_launches.push(launch_id);
        }
    }
}

/// Everything mini-app related on the session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebApps {
    /// The open calls in flight, each with what its answer is for.
    pub pending: Vec<WebAppPending>,
    /// One-shot: the open call's answer.
    pub open_result: Option<WebAppOpenResult>,
    /// Launches TDLib started for an open call that was dropped before
    /// its answer; the driver sends `closeWebApp` for each.
    pub dropped_launches: Vec<i64>,
    /// `updateAttachmentMenuBots`: the bots in the attachment menu.
    pub attachment_menu_bots: Vec<AttachmentMenuBot>,
    /// One-shot: `getAttachmentMenuBot` answer (for adding a bot), or a
    /// failure message, with the bot it was asked for: the flow waiting
    /// takes only its own bot's answer.
    pub attachment_menu_bot: Option<(i64, Result<AttachmentMenuBot, String>)>,
    /// One-shot: `toggleBotIsAddedToAttachmentMenu` outcomes, oldest
    /// first, each with the bot it answers: the flow waiting takes only its
    /// own bot's.
    pub attachment_menu_toggled: Vec<AttachmentToggle>,
    /// One-shot: write-access answers, `(bot_user_id, result)`.
    pub write_access: Option<(i64, WriteAccessResult)>,
    /// One-shot: `toggleBotCanManageEmojiStatus` outcome,
    /// `(bot_user_id, done)`.
    pub emoji_access: Option<(i64, bool)>,
    /// One-shot: `searchWebApp` answer for a `t.me/bot/app` link, the app
    /// or a failure message, with the bot it was asked for: a second link
    /// clicked before the first answered takes only its own answer.
    pub found: Option<(i64, Result<FoundWebApp, String>)>,
    /// `req_id`s of custom requests in flight, by `RequestId`.
    pub custom_requests: HashMap<u64, String>,
    /// One-shot: custom request answers.
    pub custom_replies: Vec<CustomRequestReply>,
    /// Apps tab: `getGrossingWebAppBots` bot ids (`None` until loaded).
    pub grossing_bots: Option<Vec<i64>>,
    pub grossing_loading: bool,
    /// One-shot: `sendWebAppData` outcome (`Ok` or a failure message).
    pub data_sent: Option<Result<(), String>>,
    /// One-shot: the contact card sent for `web_app_request_phone` was
    /// taken by TDLib or refused, with the request it answers.
    pub phone_share: Option<PhoneShare>,
}
