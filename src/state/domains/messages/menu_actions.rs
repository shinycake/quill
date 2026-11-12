//! What `getMessageProperties` said each message may offer, by message.
//! The menu, a gallery row's hover and the media viewer each ask about a
//! message of their own; one shared slot let the last answer overwrite the
//! others', and a failed ask left the menu on its permissive local guess.
use crate::state::*;
use crate::telegram::envelope::MessageActions;

/// The most messages remembered; the oldest answer goes first.
const KEPT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Know {
    /// Asked as this request; its answer or its error replaces this.
    Asked(RequestId),
    Allows(MessageActions),
    /// TDLib refused to say.
    Failed,
}

/// The answers by `(chat, message)`, oldest first.
#[derive(Debug, Default, Clone)]
pub struct MenuActions {
    known: Vec<((ChatId, MessageId), Know)>,
}

impl MenuActions {
    fn put(&mut self, chat_id: ChatId, message_id: MessageId, know: Know) {
        self.known.retain(|(key, _)| *key != (chat_id, message_id));
        self.known.push(((chat_id, message_id), know));
        if self.known.len() > KEPT {
            self.known.remove(0);
        }
    }

    fn find(&self, chat_id: ChatId, message_id: MessageId) -> Option<Know> {
        self.known
            .iter()
            .find(|(key, _)| *key == (chat_id, message_id))
            .map(|(_, know)| *know)
    }

    /// Facts known without asking (an injected menu).
    pub fn set(&mut self, chat_id: ChatId, message_id: MessageId, actions: MessageActions) {
        self.put(chat_id, message_id, Know::Allows(actions));
    }

    /// An ask for this message went out as `request`: a row that hovers
    /// again waits for it instead of asking once more, and only its answer
    /// or error lands. An answer already known (kept while a gallery row
    /// refreshes it) stays; a newer ask replaces an older one in flight.
    pub fn asked(&mut self, chat_id: ChatId, message_id: MessageId, request: RequestId) {
        if matches!(
            self.find(chat_id, message_id),
            None | Some(Know::Asked(_))
        ) {
            self.put(chat_id, message_id, Know::Asked(request));
        }
    }

    /// Whether `request` is still the one this message waits for: an older
    /// ask's answer, after a newer one went out, is not.
    fn is_current(&self, chat_id: ChatId, message_id: MessageId, request: RequestId) -> bool {
        !matches!(
            self.find(chat_id, message_id),
            Some(Know::Asked(waiting)) if waiting != request
        )
    }

    /// `request` answered with what TDLib allows. Whether it landed.
    pub fn answered(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        request: RequestId,
        actions: MessageActions,
    ) -> bool {
        let current = self.is_current(chat_id, message_id, request);
        if current {
            self.put(chat_id, message_id, Know::Allows(actions));
        }
        current
    }

    /// Whether an ask for this message is out.
    pub fn is_asked(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        matches!(self.find(chat_id, message_id), Some(Know::Asked(_)))
    }

    /// `request` failed: the message's gated rows stay hidden (unless a
    /// newer ask is out, which is the one that decides).
    pub fn fail(&mut self, chat_id: ChatId, message_id: MessageId, request: RequestId) {
        if self.is_current(chat_id, message_id, request) {
            self.put(chat_id, message_id, Know::Failed);
        }
    }

    /// What TDLib allows for the message; `None` until it answers.
    pub fn get(&self, chat_id: ChatId, message_id: MessageId) -> Option<MessageActions> {
        match self.find(chat_id, message_id)? {
            Know::Allows(actions) => Some(actions),
            Know::Asked(_) | Know::Failed => None,
        }
    }

    /// Whether "delete for everyone" is open for the message: TDLib's
    /// `can_be_deleted_for_all_users` once it answered, `local` (your own
    /// message) until then, and no after a failed ask.
    pub fn allows_revoke(&self, chat_id: ChatId, message_id: MessageId, local: bool) -> bool {
        match self.find(chat_id, message_id) {
            Some(Know::Allows(actions)) => actions.can_be_deleted_for_all_users,
            Some(Know::Failed) => false,
            Some(Know::Asked(_)) | None => local,
        }
    }

    pub fn is_failed(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.find(chat_id, message_id) == Some(Know::Failed)
    }

    /// The answer is out of date (the menu asks again).
    pub fn forget(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.known.retain(|(key, _)| *key != (chat_id, message_id));
    }
}

#[cfg(test)]
mod tests {
    use super::MenuActions;
    use crate::ids::{ChatId, MessageId, RequestId};
    use crate::telegram::envelope::MessageActions;

    const CHAT: ChatId = ChatId(1);
    const MESSAGE: MessageId = MessageId(2);

    #[test]
    fn an_ask_in_flight_is_waited_for_until_its_answer_or_error() {
        let mut known = MenuActions::default();
        assert!(!known.is_asked(CHAT, MESSAGE));
        known.asked(CHAT, MESSAGE, RequestId(7));
        assert!(known.is_asked(CHAT, MESSAGE));
        assert_eq!(known.get(CHAT, MESSAGE), None);
        // The local guess holds while the ask is out.
        assert!(known.allows_revoke(CHAT, MESSAGE, true));
        known.set(CHAT, MESSAGE, MessageActions::default());
        assert!(!known.is_asked(CHAT, MESSAGE));
        assert!(known.get(CHAT, MESSAGE).is_some());

        let other = MessageId(3);
        known.asked(CHAT, other, RequestId(8));
        known.fail(CHAT, other, RequestId(8));
        assert!(!known.is_asked(CHAT, other));
        assert!(known.is_failed(CHAT, other));
    }

    #[test]
    fn an_older_ask_cannot_replace_a_newer_one_in_either_order() {
        let allows = MessageActions {
            can_be_copied: true,
            ..MessageActions::default()
        };
        // The older answer comes first and is dropped; the newer lands.
        let mut known = MenuActions::default();
        known.asked(CHAT, MESSAGE, RequestId(1));
        known.asked(CHAT, MESSAGE, RequestId(2));
        assert!(!known.answered(CHAT, MESSAGE, RequestId(1), MessageActions::default()));
        assert!(known.is_asked(CHAT, MESSAGE));
        assert!(known.answered(CHAT, MESSAGE, RequestId(2), allows));
        assert_eq!(known.get(CHAT, MESSAGE), Some(allows));

        // The newer fails first; the older one then answers with real
        // data, which lands (nothing newer is waiting any more).
        let mut known = MenuActions::default();
        known.asked(CHAT, MESSAGE, RequestId(1));
        known.asked(CHAT, MESSAGE, RequestId(2));
        known.fail(CHAT, MESSAGE, RequestId(2));
        assert!(known.is_failed(CHAT, MESSAGE));
        assert!(known.answered(CHAT, MESSAGE, RequestId(1), allows));
        assert_eq!(known.get(CHAT, MESSAGE), Some(allows));

        // An older error cannot replace a newer ask.
        let mut known = MenuActions::default();
        known.asked(CHAT, MESSAGE, RequestId(1));
        known.asked(CHAT, MESSAGE, RequestId(2));
        known.fail(CHAT, MESSAGE, RequestId(1));
        assert!(known.is_asked(CHAT, MESSAGE));
    }

    #[test]
    fn a_refresh_keeps_the_answer_it_already_has() {
        let mut known = MenuActions::default();
        known.set(CHAT, MESSAGE, MessageActions::default());
        known.asked(CHAT, MESSAGE, RequestId(9));
        assert!(!known.is_asked(CHAT, MESSAGE));
        assert!(known.get(CHAT, MESSAGE).is_some());
    }
}
