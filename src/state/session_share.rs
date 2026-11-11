//! Share box and "send as" state.
use super::*;

/// One half of the share box search: its request in flight, or its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchPart {
    Waiting(RequestId),
    Done(Vec<i64>),
}

impl SearchPart {
    fn ids(&self) -> &[i64] {
        match self {
            Self::Waiting(_) => &[],
            Self::Done(ids) => ids,
        }
    }
}

/// Share box search: no query, or the trimmed query with its `searchChats`
/// and `searchChatsOnServer` requests and answers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ShareSearch {
    #[default]
    Idle,
    Searching {
        /// Trimmed query the answers belong to.
        query: String,
        /// `searchChats` answer, in TDLib order.
        local: SearchPart,
        /// `searchChatsOnServer` answer, in TDLib order.
        server: SearchPart,
    },
}

impl ShareSearch {
    /// Start a new search; earlier answers and requests are dropped.
    pub fn begin(&mut self, query: &str, local: RequestId, server: RequestId) {
        *self = Self::Searching {
            query: query.trim().to_string(),
            local: SearchPart::Waiting(local),
            server: SearchPart::Waiting(server),
        };
    }

    /// Forget the search (empty query).
    pub fn clear(&mut self) {
        *self = Self::Idle;
    }

    /// The query the answers belong to; empty when there is no search.
    pub fn query(&self) -> &str {
        match self {
            Self::Idle => "",
            Self::Searching { query, .. } => query,
        }
    }

    /// `searchChats` answer so far.
    pub fn local(&self) -> &[i64] {
        match self {
            Self::Idle => &[],
            Self::Searching { local, .. } => local.ids(),
        }
    }

    /// `searchChatsOnServer` answer so far.
    pub fn server(&self) -> &[i64] {
        match self {
            Self::Idle => &[],
            Self::Searching { server, .. } => server.ids(),
        }
    }

    /// Take an answer when it belongs to the current search; late answers
    /// of an older query are ignored.
    pub fn accept(&mut self, request: RequestId, chat_ids: &[ChatId]) {
        let Self::Searching { local, server, .. } = self else {
            return;
        };
        let ids = chat_ids.iter().map(|id| id.0).collect();
        if *local == SearchPart::Waiting(request) {
            *local = SearchPart::Done(ids);
        } else if *server == SearchPart::Waiting(request) {
            *server = SearchPart::Done(ids);
        }
    }

    /// Whether an answer is still outstanding.
    pub fn is_searching(&self) -> bool {
        match self {
            Self::Idle => false,
            Self::Searching { local, server, .. } => {
                matches!(local, SearchPart::Waiting(_)) || matches!(server, SearchPart::Waiting(_))
            }
        }
    }
}

impl Session {
    /// A chat the share box can send to: supported, and not a channel where
    /// the user is known to be unable to post.
    pub fn can_share_to(chat: &ChatSummary) -> bool {
        chat.supported()
            && (!chat.is_channel()
                || chat.member_status().is_none()
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
        // tdesktop's forward box lists Saved Messages first
        // (`ShowForwardMessagesBox` pushes `session->user()` before the
        // dialogs).
        if let Some(index) = result
            .iter()
            .position(|chat| self.is_saved_messages(chat.id))
            && let Some(saved) = (index > 0).then(|| result.remove(index))
        {
            result.insert(0, saved);
        }
        if query.trim().is_empty() || self.messages.share_search.query() != query.trim() {
            return result;
        }
        let mut seen: HashSet<i64> = result.iter().map(|chat| chat.id.0).collect();
        for id in self
            .messages
            .share_search
            .local()
            .iter()
            .chain(self.messages.share_search.server().iter())
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

    /// The second line of a share box row (tdesktop `PeerListRow` status):
    /// presence for a person, "bot" for a bot, the member count for a group
    /// or channel, nothing for Saved Messages. The flag marks "online",
    /// which tdesktop draws in the accent color.
    pub fn share_row_status(&self, chat: &ChatSummary) -> Option<(String, bool)> {
        use crate::telegram::envelope::ChatKind;
        if self.is_saved_messages(chat.id) {
            return None;
        }
        match chat.kind {
            ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => {
                let user = self.user(user_id.0)?;
                if user.is_bot {
                    return Some((
                        crate::lang::tr(crate::lang::keys::STATUS_BOT).to_string(),
                        false,
                    ));
                }
                let line = user.status.display();
                (!line.is_empty()).then(|| (line, user.status.is_online()))
            }
            ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. } => {
                let (members, _) = self.group_member_counts(chat)?;
                (members > 0).then(|| {
                    (
                        crate::inline_members::member_status_line(members, chat.is_channel(), 0),
                        false,
                    )
                })
            }
            ChatKind::Unknown => None,
        }
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

    /// The loaded `getChatAvailableMessageSenders` list, if any (the
    /// previous one while a refetch runs or after one failed).
    pub fn available_message_senders(&self, chat_id: ChatId) -> &[AvailableMessageSender] {
        self.chats_state
            .send_as_options
            .get(&chat_id.0)
            .and_then(crate::state::fetch::Fetch::data)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Where the sender list of the chat stands.
    pub fn message_senders_fetch(
        &self,
        chat_id: ChatId,
    ) -> &crate::state::fetch::Fetch<Vec<AvailableMessageSender>> {
        const IDLE: &crate::state::fetch::Fetch<Vec<AvailableMessageSender>> =
            &crate::state::fetch::Fetch::Idle;
        self.chats_state
            .send_as_options
            .get(&chat_id.0)
            .unwrap_or(IDLE)
    }

    /// The list was asked for: the picker shows a spinner, over the
    /// previous list if there was one.
    pub fn message_senders_loading(&mut self, chat_id: ChatId, request: RequestId) {
        let slot = self
            .chats_state
            .send_as_options
            .entry(chat_id.0)
            .or_default();
        slot.begin();
        slot.sent(request);
    }

    /// The list `request` asked for arrived: taken only by the fetch that
    /// waits for it (an older request's answer, or one after the lists
    /// were dropped, is ignored).
    pub(crate) fn accept_message_senders(
        &mut self,
        chat_id: ChatId,
        request: Option<RequestId>,
        senders: Vec<AvailableMessageSender>,
    ) {
        if let Some(slot) = self.chats_state.send_as_options.get_mut(&chat_id.0) {
            slot.answered(request, senders);
        }
    }

    /// The request for the list failed: the picker offers Retry. Only the
    /// request the fetch waits for fails it.
    pub fn fail_message_senders(
        &mut self,
        chat_id: ChatId,
        request: Option<RequestId>,
        error: String,
    ) {
        if let Some(slot) = self.chats_state.send_as_options.get_mut(&chat_id.0) {
            slot.failed(request, error, false);
        }
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

#[cfg(test)]
#[path = "share_rows_tests.rs"]
mod share_rows_tests;
