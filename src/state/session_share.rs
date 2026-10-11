//! Share box and "send as" state.
use super::*;

/// Share box search: the typed query, the in-flight `searchChats` /
/// `searchChatsOnServer` requests and their answers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShareSearch {
    /// Trimmed query the answers belong to.
    pub query: String,
    local_request: Option<RequestId>,
    server_request: Option<RequestId>,
    /// `searchChats` answer, in TDLib order.
    pub local: Vec<i64>,
    /// `searchChatsOnServer` answer, in TDLib order.
    pub server: Vec<i64>,
}

impl ShareSearch {
    /// Start a new search; earlier answers and requests are dropped.
    pub fn begin(&mut self, query: &str, local: RequestId, server: RequestId) {
        self.query = query.trim().to_string();
        self.local_request = Some(local);
        self.server_request = Some(server);
        self.local.clear();
        self.server.clear();
    }

    /// Forget the search (empty query).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Take an answer when it belongs to the current search; late answers
    /// of an older query are ignored.
    pub fn accept(&mut self, request: RequestId, chat_ids: &[ChatId]) {
        let ids = chat_ids.iter().map(|id| id.0).collect();
        if self.local_request == Some(request) {
            self.local_request = None;
            self.local = ids;
        } else if self.server_request == Some(request) {
            self.server_request = None;
            self.server = ids;
        }
    }

    /// Whether an answer is still outstanding.
    pub fn is_searching(&self) -> bool {
        self.local_request.is_some() || self.server_request.is_some()
    }
}

impl Session {
    /// A chat the share box can send to: supported, and not a channel where
    /// the user is known to be unable to post.
    fn can_share_to(chat: &ChatSummary) -> bool {
        chat.supported()
            && (!chat.is_channel()
                || chat.my_member_status.is_none()
                || chat.channel_admin_can_post())
    }

    /// Share box rows for `query`: loaded Main-list chats whose title matches,
    /// then chats only the server search found (tdesktop's share box merges
    /// the global result list below the dialogs).
    pub fn share_destinations(&self, query: &str) -> Vec<&ChatSummary> {
        let mut result: Vec<&ChatSummary> = self
            .local_search_chats(query)
            .into_iter()
            .filter(|chat| Self::can_share_to(chat))
            .collect();
        if query.trim().is_empty() || self.messages.share_search.query != query.trim() {
            return result;
        }
        let mut seen: HashSet<i64> = result.iter().map(|chat| chat.id.0).collect();
        for id in self
            .messages
            .share_search
            .local
            .iter()
            .chain(self.messages.share_search.server.iter())
        {
            if let Some(chat) = self.chats.get(id)
                && Self::can_share_to(chat)
                && seen.insert(*id)
            {
                result.push(chat);
            }
        }
        result
    }

    /// Distinct original senders of the forwarded messages, in order, for
    /// the forward bar's first line (tdesktop `ForwardPanel::checkTexts`).
    pub fn forward_bar_senders(&self, chat_id: ChatId, ids: &[MessageId]) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for id in ids {
            let Some(message) = self
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.get(&id.0))
            else {
                continue;
            };
            let name = match &message.forward_info {
                Some(info) => self.origin_name_and_accent(&info.origin).0,
                None if message.is_outgoing => Some("You".to_string()),
                None => self
                    .sender_name_and_accent(message.sender)
                    .0
                    .or_else(|| self.chats.get(&chat_id.0).map(|chat| chat.title.clone())),
            };
            if let Some(name) = name
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        names
    }

    /// One-line preview of a forwarded message for the forward bar.
    pub fn forward_bar_preview(&self, chat_id: ChatId, id: MessageId) -> String {
        self.histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&id.0))
            .map(effective_preview)
            .unwrap_or_else(|| "Message".into())
    }

    pub(crate) fn set_chat_message_sender(&mut self, chat_id: i64, sender: Option<MessageSender>) {
        match sender {
            Some(sender) => {
                self.chats_state.chat_message_sender.insert(chat_id, sender);
            }
            None => {
                self.chats_state.chat_message_sender.remove(&chat_id);
            }
        }
    }

    /// The "send as" identity selected in the chat; `None` when the user
    /// cannot change it (the picker is hidden then).
    pub fn selected_message_sender(&self, chat_id: ChatId) -> Option<MessageSender> {
        self.chats_state
            .chat_message_sender
            .get(&chat_id.0)
            .copied()
    }

    /// Whether the composer shows the send-as button: TDLib reported a
    /// selectable sender for the chat (`chat.message_sender_id`).
    pub fn can_choose_message_sender(&self, chat_id: ChatId) -> bool {
        self.chats_state
            .chat_message_sender
            .contains_key(&chat_id.0)
    }

    /// The loaded `getChatAvailableMessageSenders` list, if any.
    pub fn available_message_senders(&self, chat_id: ChatId) -> &[AvailableMessageSender] {
        self.chats_state
            .send_as_options
            .get(&chat_id.0)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Display name of a sender for the picker. Users resolve through the
    /// private chat, chats through their own title.
    pub fn message_sender_title(&self, sender: MessageSender) -> String {
        match sender {
            MessageSender::Chat { chat_id } => self
                .chats
                .get(&chat_id)
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| "Channel".into()),
            MessageSender::User { user_id } => self
                .user(user_id)
                .map(|user| user.display_name())
                .unwrap_or_else(|| "You".into()),
        }
    }

    /// Status line under a sender in the picker (tdesktop
    /// `ChooseSendAsBox`): "Anonymous admin" for the group itself, the
    /// subscriber line is left to the title, premium-locked ones say so.
    pub fn message_sender_status(
        &self,
        open_chat: ChatId,
        entry: &AvailableMessageSender,
    ) -> &'static str {
        if entry.needs_premium {
            "Telegram Premium required"
        } else {
            match entry.sender {
                MessageSender::Chat { chat_id } if chat_id == open_chat.0 => "Anonymous admin",
                MessageSender::Chat { .. } => "Channel",
                MessageSender::User { .. } => "Personal account",
            }
        }
    }
}
