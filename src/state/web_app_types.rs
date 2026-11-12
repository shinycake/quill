//! Mini-app state the reducer keeps for the UI: one-shot answers the UI
//! takes on its next poll, plus the attachment menu bots.

use crate::telegram::envelope::{AttachmentMenuBot, FoundWebApp};
use crate::web_app::LaunchSource;
use std::collections::HashMap;

/// The open call in flight (`openWebApp`, `getWebAppUrl`, `getMainWebApp`,
/// `getWebAppLinkUrl`): what the answer is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAppPending {
    /// The open call this is for: only its answer opens the launch.
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

/// A write-access answer: for which bot, to which request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteAnswer {
    pub request: crate::ids::RequestId,
    pub bot_user_id: i64,
    pub result: WriteAccessResult,
}

impl WriteAnswer {
    pub fn new(
        request: crate::ids::RequestId,
        bot_user_id: i64,
        result: WriteAccessResult,
    ) -> Self {
        Self {
            request,
            bot_user_id,
            result,
        }
    }
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

/// The answer of one `toggleBotCanManageEmojiStatus`, by the request that
/// asked: only the flow waiting for that request takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmojiAccessAnswer {
    pub request: crate::ids::RequestId,
    pub result: EmojiAccessResult,
}

/// What `toggleBotCanManageEmojiStatus` came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmojiAccessResult {
    Granted,
    Refused,
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

/// The outcome of one `toggleBotIsAddedToAttachmentMenu`, by the request
/// that sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentToggle {
    /// The `toggleBotIsAddedToAttachmentMenu` call this answers.
    pub request: crate::ids::RequestId,
    pub bot_user_id: i64,
    /// Whether the bot is in the menu now, or why the change failed.
    pub result: Result<bool, String>,
}

/// `sendWebAppCustomRequest` answers, keyed by the app's `req_id` and the
/// launch that asked: another app's window never takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRequestReply {
    pub launch_id: i64,
    pub req_id: String,
    /// The JSON result, or an error word for the app.
    pub result: Result<String, String>,
}

impl WebApps {
    /// The mini app launch `launch_id` closed: its custom requests in
    /// flight and the answers nobody took go with it.
    pub fn forget_custom_requests_of(&mut self, launch_id: i64) {
        self.custom_requests
            .retain(|_, (launch, _)| *launch != launch_id);
        self.custom_replies
            .retain(|reply| reply.launch_id != launch_id);
    }

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

/// The outcome of one `sendWebAppData`, by the request that sent it: only
/// the flow that sent that request reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSent {
    pub request: crate::ids::RequestId,
    /// `Ok` or a failure message.
    pub result: Result<(), String>,
}

/// Everything mini-app related on the session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebApps {
    /// The open call in flight and what its answer is for. A newer launch
    /// replaces it: the older answer opens nothing.
    pub opening: Option<WebAppPending>,
    /// One-shot: the open call's answer.
    pub open_result: Option<WebAppOpenResult>,
    /// Launches TDLib started for an open call that was dropped before
    /// its answer; the driver sends `closeWebApp` for each.
    pub dropped_launches: Vec<i64>,
    /// `updateAttachmentMenuBots`: the bots in the attachment menu.
    pub attachment_menu_bots: Vec<AttachmentMenuBot>,
    /// One-shot: `getAttachmentMenuBot` answers (for adding a bot), or a
    /// failure message, oldest first, each with the request it answers: the
    /// flow waiting takes only the answer of its own request.
    pub attachment_menu_bot: Vec<(crate::ids::RequestId, Result<AttachmentMenuBot, String>)>,
    /// One-shot: `toggleBotIsAddedToAttachmentMenu` outcomes, oldest
    /// first, each with the request it answers: the flow waiting takes only
    /// its own request's.
    pub attachment_menu_toggled: Vec<AttachmentToggle>,
    /// One-shot: the write-access answer, with the request it answers.
    pub write_access: Option<WriteAnswer>,
    /// One-shot: `toggleBotCanManageEmojiStatus` outcomes, oldest first,
    /// each with its request: the mini app waiting for a grant takes only
    /// its own request's, not the bot permissions panel's.
    pub emoji_access: Vec<EmojiAccessAnswer>,
    /// One-shot: `searchWebApp` answers for `t.me/bot/app` links, the app
    /// or a failure message, oldest first, each with the request it
    /// answers: a second link of the same bot clicked before the first
    /// answered takes only its own answer.
    pub found: Vec<(crate::ids::RequestId, Result<FoundWebApp, String>)>,
    /// `(launch_id, req_id)` of custom requests in flight, by `RequestId`.
    pub custom_requests: HashMap<u64, (i64, String)>,
    /// One-shot: custom request answers.
    pub custom_replies: Vec<CustomRequestReply>,
    /// Apps tab: `getGrossingWebAppBots` bot ids. One request at a time,
    /// so its answer and its error need no key; a failure is drawn with
    /// the tab's reopen as the retry.
    pub grossing: crate::state::fetch::Fetch<Vec<i64>>,
    /// One-shot: `sendWebAppData` outcomes, oldest first, each by its
    /// request.
    pub data_sent: Vec<DataSent>,
    /// One-shot: the contact card sent for `web_app_request_phone` was
    /// taken by TDLib or refused, with the request it answers.
    pub phone_share: Option<PhoneShare>,
}
