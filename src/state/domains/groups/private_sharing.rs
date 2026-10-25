use crate::state::*;
use crate::telegram::envelope::{MessageContent, ServiceAction};

const RECENTLY_DISABLED_SECS: i64 = 86_400;

/// Enable Sharing, once TDLib said who protects the chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnableSharing {
    /// Nobody else protects it: the driver turns protection off next.
    TurnOff(ChatId),
    /// The other person protects it, so TDLib will send them a request:
    /// the "Send Request?" box asks first.
    AskToRequest(ChatId),
}

#[derive(Debug, Default)]
pub struct PrivateSharingState {
    pub enable: Option<EnableSharing>,
    disabled_at_unix: HashMap<i64, i64>,
}

impl PrivateSharingState {
    pub(crate) fn record_disabled(&mut self, chat_id: ChatId, now: i64) {
        self.disabled_at_unix.insert(chat_id.0, now);
    }

    pub(crate) fn forget_disabled(&mut self, chat_id: ChatId) {
        self.disabled_at_unix.remove(&chat_id.0);
    }

    pub fn recently_disabled_by_me(&self, chat_id: ChatId, now: i64) -> bool {
        self.disabled_at_unix
            .get(&chat_id.0)
            .is_some_and(|at| now.saturating_sub(*at) < RECENTLY_DISABLED_SECS)
    }

    pub fn request_box_chat(&self) -> Option<ChatId> {
        match self.enable {
            Some(EnableSharing::AskToRequest(chat_id)) => Some(chat_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharingRequestView {
    /// The card listing what sharing includes; without it the row is the
    /// "has expired" line.
    pub card: bool,
    /// Reject and Accept under the card.
    pub buttons: bool,
}

impl Session {
    /// tdesktop `HistoryItem::setServiceMessageByAction` for
    /// `messageActionNoForwardsRequest`: an expired request keeps its card
    /// once answered, and only an incoming live one can be answered.
    pub fn sharing_request_view(&self, message: &HistoryMessage) -> Option<SharingRequestView> {
        let MessageContent::Action(action) = &message.content else {
            return None;
        };
        let ServiceAction::ProtectedDisableRequested { expired } = action.as_ref() else {
            return None;
        };
        let answered = self.sharing_request_answered(message.chat_id, message.id);
        Some(SharingRequestView {
            card: !expired || answered,
            buttons: !expired && !message.is_outgoing,
        })
    }

    fn sharing_request_answered(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        let Some(history) = self.histories.get(&chat_id.0) else {
            return false;
        };
        let mut later = history
            .messages
            .range(message_id.0.saturating_add(1)..)
            .map(|(_, message)| message);
        let next_is_toggle = later.next().and_then(toggle_request_id).is_some();
        next_is_toggle || later.any(|message| toggle_request_id(message) == Some(message_id.0))
    }

    pub fn sharing_request_answering(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.requests.has_purpose(RequestPurpose::Groups(
            GroupsPurpose::AnswerSharingRequest {
                chat_id,
                message_id,
            },
        ))
    }
}

fn toggle_request_id(message: &HistoryMessage) -> Option<i64> {
    match &message.content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::ProtectedToggled {
                request_message_id, ..
            } => Some(*request_message_id),
            _ => None,
        },
        _ => None,
    }
}
