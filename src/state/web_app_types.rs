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
    /// Drains every phone-share answer. The one for `waiting` (the send
    /// the open window's flow waits on) is returned as "was it sent";
    /// the rest belong to a send nobody waits on (timed out, window
    /// closed) and are dropped, so none outlives the next poll.
    pub fn take_phone_share(&mut self, waiting: Option<crate::ids::RequestId>) -> Option<bool> {
        let answers = std::mem::take(&mut self.phone_share);
        let waiting = waiting?;
        answers
            .into_iter()
            .find(|answer| answer.request() == waiting)
            .map(PhoneShare::is_sent)
    }

    /// The mini app launch `launch_id` closed: its custom requests in
    /// flight and the answers nobody took go with it.
    pub fn forget_custom_requests_of(&mut self, launch_id: i64) {
        self.custom_requests
            .retain(|_, (launch, _)| *launch != launch_id);
        self.custom_replies
            .retain(|reply| reply.launch_id != launch_id);
    }

    /// The account leaves the screen: every launch still waiting and every
    /// answer nobody took goes in one place, so no slot outlives the account
    /// (the windows that waited for them go with it). An `Opened` answer
    /// not yet shown is queued for `closeWebApp`.
    pub fn forget_answers(&mut self) {
        self.clear_launch();
        self.found.clear();
        self.attachment_menu_bot.clear();
        self.attachment_menu_toggled.clear();
        self.write_access.clear();
        self.emoji_access.clear();
        self.data_sent.clear();
        self.phone_share.clear();
        self.custom_requests.clear();
        self.custom_replies.clear();
    }

    /// A launch was asked: it replaces the one before, whose answer was
    /// never shown or whose ask is dropped.
    pub fn begin_launch(&mut self, pending: WebAppPending) {
        self.set_launch(OpenLaunch::Asking(pending));
    }

    /// The launch goes (the account left the screen, a send was refused).
    pub fn clear_launch(&mut self) {
        self.set_launch(OpenLaunch::Idle);
    }

    /// The open call in flight, and what its answer is for.
    pub fn opening(&self) -> Option<&WebAppPending> {
        match &self.launch {
            OpenLaunch::Asking(pending) => Some(pending),
            _ => None,
        }
    }

    /// The open call `request` answered or failed: its context, when it is
    /// still the launch asked (an older one's answer opens nothing).
    pub fn settle_launch(&mut self, request: crate::ids::RequestId) -> Option<WebAppPending> {
        match std::mem::take(&mut self.launch) {
            OpenLaunch::Asking(pending) if pending.request == request => Some(pending),
            other => {
                self.launch = other;
                None
            }
        }
    }

    /// The open call's answer, for the poll to show.
    pub fn answer_launch(&mut self, result: WebAppOpenResult) {
        self.set_launch(OpenLaunch::Answered(result));
    }

    /// The answer the poll has not taken yet.
    pub fn open_result(&self) -> Option<&WebAppOpenResult> {
        match &self.launch {
            OpenLaunch::Answered(result) => Some(result),
            _ => None,
        }
    }

    /// The poll takes the answer.
    pub fn take_open_result(&mut self) -> Option<WebAppOpenResult> {
        match std::mem::take(&mut self.launch) {
            OpenLaunch::Answered(result) => Some(result),
            other => {
                self.launch = other;
                None
            }
        }
    }

    /// Replace the launch. An `Opened` answer it pushes out was never
    /// shown, but TDLib started that app: its launch is queued for
    /// `closeWebApp`, so the bot keeps no live session.
    fn set_launch(&mut self, next: OpenLaunch) {
        if let OpenLaunch::Answered(WebAppOpenResult::Opened { launch_id, .. }) =
            std::mem::replace(&mut self.launch, next)
            && launch_id != 0
        {
            self.dropped_launches.push(launch_id);
        }
    }
}

/// One mini-app launch, from the open call to the poll showing its
/// answer. Leaves with the answer (`take_open_result`), a newer launch, the
/// send being refused or the account leaving the screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OpenLaunch {
    #[default]
    Idle,
    /// The open call is out; its context holds the request.
    Asking(WebAppPending),
    /// Answered, and not shown yet.
    Answered(WebAppOpenResult),
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
    /// The current launch: the open call in flight and what its answer is
    /// for, then its answer until the poll takes it. A newer launch
    /// replaces it: the older answer opens nothing.
    pub launch: OpenLaunch,
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
    /// One-shot: write-access answers, oldest first, each with the request
    /// it answers. Two mini apps asking before the poll runs keep both.
    pub write_access: Vec<WriteAnswer>,
    /// One-shot: `toggleBotCanManageEmojiStatus` outcomes, oldest first,
    /// each by its request: the mini app waiting for a grant takes only its
    /// own request's, not the bot permissions panel's, and two answers in
    /// one poll are both kept.
    pub emoji_access: Vec<EmojiAccessAnswer>,
    /// One-shot: `searchWebApp` answers for `t.me/bot/app` links, the app
    /// or a failure message, oldest first, each with the request it
    /// answers: a second link of the same bot clicked before the first
    /// answered takes only its own answer.
    pub found: Vec<(crate::ids::RequestId, Result<FoundWebApp, String>)>,
    /// `(launch_id, req_id)` of custom requests in flight, by `RequestId`.
    pub custom_requests: HashMap<crate::ids::RequestId, (i64, String)>,
    /// One-shot: custom request answers.
    pub custom_replies: Vec<CustomRequestReply>,
    /// Apps tab: `getGrossingWebAppBots` bot ids. One request at a time,
    /// so its answer and its error are matched to the request its stage
    /// holds (`answered` / `failed`); a failure is drawn with the tab's
    /// reopen as the retry.
    pub grossing: crate::state::fetch::Fetch<Vec<i64>>,
    /// One-shot: `sendWebAppData` outcomes, oldest first, each by its
    /// request.
    pub data_sent: Vec<DataSent>,
    /// One-shot: contact cards sent for `web_app_request_phone` that TDLib
    /// took or refused, oldest first, each with the request it answers.
    pub phone_share: Vec<PhoneShare>,
}

#[cfg(test)]
mod forget_tests {
    use super::*;
    use crate::ids::RequestId;

    /// 10-004: leaving the account empties every answer slot, not only
    /// some of them.
    #[test]
    fn forgetting_the_launches_empties_every_answer_slot() {
        let mut apps = WebApps::default();
        apps.data_sent.push(DataSent {
            request: RequestId(1),
            result: Ok(()),
        });
        apps.custom_requests.insert(RequestId(2), (7, "req".into()));
        apps.custom_replies.push(CustomRequestReply {
            launch_id: 7,
            req_id: "req".into(),
            result: Ok("{}".into()),
        });
        apps.found
            .push((RequestId(3), Err("no such app".to_string())));
        apps.attachment_menu_bot
            .push((RequestId(4), Err("no".to_string())));
        apps.forget_answers();
        assert_eq!(apps, WebApps::default());
    }

    fn asked(request: u64) -> WebAppPending {
        WebAppPending {
            request: RequestId(request),
            bot_user_id: 5,
            chat_id: None,
            source: LaunchSource::MenuButton,
        }
    }

    #[test]
    fn a_launch_is_one_value_from_the_ask_to_the_shown_answer() {
        let mut apps = WebApps::default();
        apps.begin_launch(asked(1));
        assert!(apps.opening().is_some());
        // An older request's answer opens nothing and keeps the ask.
        assert!(apps.settle_launch(RequestId(9)).is_none());
        assert!(apps.opening().is_some());
        assert!(apps.settle_launch(RequestId(1)).is_some());
        assert!(apps.opening().is_none());
        apps.answer_launch(WebAppOpenResult::Opened {
            bot_user_id: 5,
            chat_id: None,
            source: LaunchSource::MenuButton,
            launch_id: 77,
            url: "https://example.com".into(),
        });
        assert!(apps.open_result().is_some());
        // A newer launch drops the answer nobody showed and closes its app.
        apps.begin_launch(asked(2));
        assert!(apps.open_result().is_none());
        assert_eq!(apps.dropped_launches, vec![77]);
        assert!(apps.take_open_result().is_none());
        assert!(apps.opening().is_some(), "taking nothing keeps the ask");
    }
}
