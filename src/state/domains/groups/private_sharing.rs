use crate::state::*;
use crate::telegram::envelope::{MessageContent, ServiceAction};

const RECENTLY_DISABLED_SECS: i64 = 86_400;

/// Enable Sharing: nothing asked, the lookup of who protects the chat out,
/// or what TDLib's answer (or its error) moved the flow to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EnableSharing {
    #[default]
    Idle,
    /// `getMessageProperties` for `chat` is out as `request`: only its
    /// answer or error leaves the stage (the box closing does too).
    Asking { chat: ChatId, request: RequestId },
    /// Nobody else protects it: the driver turns protection off next.
    TurnOff(ChatId),
    /// The other person protects it, so TDLib will send them a request:
    /// the "Send Request?" box asks first.
    AskToRequest(ChatId),
}

/// An Accept or Reject under an incoming sharing request, out as `request`:
/// the only record of it, so its answer, its error or its loss frees the
/// buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SharingAnswer {
    chat_id: ChatId,
    message_id: MessageId,
    request: RequestId,
}

#[derive(Debug, Default)]
pub struct PrivateSharingState {
    pub enable: EnableSharing,
    disabled_at_unix: HashMap<i64, i64>,
    answering: Vec<SharingAnswer>,
}

impl PrivateSharingState {
    /// An answer to the sharing request `message_id` of `chat_id` went out
    /// as `request`.
    pub(crate) fn answer_sent(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        request: RequestId,
    ) {
        self.answering.push(SharingAnswer {
            chat_id,
            message_id,
            request,
        });
    }

    /// `request` was answered, refused or lost: its buttons are free.
    pub(crate) fn answer_settled(&mut self, request: RequestId) {
        self.answering.retain(|answer| answer.request != request);
    }

    /// An answer to this sharing request is on its way.
    pub fn is_answering(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.answering
            .iter()
            .any(|answer| answer.chat_id == chat_id && answer.message_id == message_id)
    }

    pub(crate) fn record_disabled(&mut self, chat_id: ChatId, now: i64) {
        self.disabled_at_unix.insert(chat_id.0, now);
    }

    pub(crate) fn disabled_at(&self, chat_id: ChatId) -> Option<i64> {
        self.disabled_at_unix.get(&chat_id.0).copied()
    }

    /// Put back the record a refused request replaced.
    pub(crate) fn restore_disabled(&mut self, chat_id: ChatId, at: Option<i64>) {
        match at {
            Some(at) => self.record_disabled(chat_id, at),
            None => self.forget_disabled(chat_id),
        }
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
            EnableSharing::AskToRequest(chat_id) => Some(chat_id),
            _ => None,
        }
    }

    /// Whether the lookup for `chat` is out.
    pub fn asking(&self, chat: ChatId) -> bool {
        matches!(self.enable, EnableSharing::Asking { chat: asked, .. } if asked == chat)
    }

    /// The answer of `request` says who protects the chat: it moves the
    /// flow only while the flow waits for exactly that request.
    pub(crate) fn answered(&mut self, request: RequestId, other_user_protects: bool) {
        if let EnableSharing::Asking {
            chat,
            request: asked,
        } = self.enable
            && asked == request
        {
            self.enable = if other_user_protects {
                EnableSharing::AskToRequest(chat)
            } else {
                EnableSharing::TurnOff(chat)
            };
        }
    }

    /// The lookup `request` failed: who protects the chat is unknown, so
    /// the "Send Request?" box asks first (only for the request waited for).
    pub(crate) fn lookup_failed(&mut self, request: RequestId) {
        if let EnableSharing::Asking {
            chat,
            request: asked,
        } = self.enable
            && asked == request
        {
            self.enable = EnableSharing::AskToRequest(chat);
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
        self.groups
            .private_sharing
            .is_answering(chat_id, message_id)
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
