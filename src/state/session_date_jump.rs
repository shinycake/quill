//! Find in history: the calendar box, date jumps and the "From:" picker.
use super::*;
use crate::search_filters::SearchMediaKind;
use crate::telegram::envelope::{CalendarDay, ParsedChatMember};

impl Session {
    /// Open the calendar box for the open chat. `None` when no chat is open.
    pub fn open_history_calendar(&mut self, now: i64) -> Option<&HistoryCalendar> {
        let chat_id = self.open_chat?;
        let media = if self.chat_search.open && self.chat_search.chat_id == Some(chat_id) {
            self.chat_search.media
        } else {
            SearchMediaKind::All
        };
        let generation = self
            .history_calendar
            .as_ref()
            .map_or(1, |c| c.generation.saturating_add(1));
        self.date_jump_note = None;
        self.history_calendar = Some(HistoryCalendar::new(chat_id, media, now, generation));
        self.history_calendar.as_ref()
    }

    pub fn close_history_calendar(&mut self) {
        self.history_calendar = None;
    }

    pub(crate) fn apply_message_calendar(
        &mut self,
        days: Vec<CalendarDay>,
        pending: Option<&PendingRequest>,
    ) {
        let Some(RequestPurpose::GetChatMessageCalendar { generation }) =
            pending.map(|p| p.purpose)
        else {
            return;
        };
        let chat_id = pending.and_then(|p| p.chat_id);
        if let Some(calendar) = self
            .history_calendar
            .as_mut()
            .filter(|c| c.generation == generation && Some(c.chat_id) == chat_id)
        {
            calendar.accept(days);
        }
    }

    pub(crate) fn fail_message_calendar(&mut self, pending: Option<&PendingRequest>) {
        let Some(RequestPurpose::GetChatMessageCalendar { generation }) =
            pending.map(|p| p.purpose)
        else {
            return;
        };
        if let Some(calendar) = self
            .history_calendar
            .as_mut()
            .filter(|c| c.generation == generation)
        {
            calendar.fail();
        }
    }

    /// `getChatMessageByDate` answered: the last message before the day.
    pub(crate) fn accept_date_message(&mut self, message_id: MessageId) {
        self.date_jump = Some((message_id, DateJumpMode::Next));
    }

    /// `getChatMessageByDate` failed: a 404 means nothing is that old, so
    /// the jump goes to the start of the chat.
    pub(crate) fn fail_date_jump(&mut self, not_found: bool) {
        if not_found {
            self.date_jump = Some((FIRST_MESSAGE_ID, DateJumpMode::Oldest));
        } else {
            self.date_jump_note = Some("Couldn't jump to that date.".into());
        }
    }

    /// Start a date jump: the same load-around pipeline as a search hit,
    /// then retarget by `mode`.
    pub fn begin_date_jump(
        &mut self,
        message_id: MessageId,
        mode: DateJumpMode,
    ) -> ChatSearchJumpNeed {
        self.date_jump_pending = (mode != DateJumpMode::Exact).then_some((message_id, mode));
        let need = self.begin_chat_search_jump(message_id);
        if need == ChatSearchJumpNeed::AlreadyReady {
            self.resolve_date_jump(message_id);
        }
        need
    }

    /// The window around `message_id` is loaded: land on the day's first
    /// message (`Next`) or the chat's oldest loaded one (`Oldest`).
    pub(crate) fn resolve_date_jump(&mut self, message_id: MessageId) {
        let Some((pending_id, mode)) = self.date_jump_pending else {
            return;
        };
        if pending_id != message_id {
            return;
        }
        self.date_jump_pending = None;
        let Some(chat_id) = self.chat_search.chat_id.or(self.open_chat) else {
            return;
        };
        let Some(history) = self.histories.get(&chat_id.0) else {
            return;
        };
        let target = match mode {
            DateJumpMode::Exact => Some(message_id.0),
            DateJumpMode::Next => history
                .messages
                .range(message_id.0.saturating_add(1)..)
                .next()
                .map(|(id, _)| *id),
            DateJumpMode::Oldest => history.messages.keys().next().copied(),
        };
        if let Some(id) = target.filter(|id| history.contains(MessageId(*id))) {
            self.chat_search.jump = ChatSearchJump::Ready {
                message_id: MessageId(id),
            };
            self.chat_search.jump_serial += 1;
        }
    }

    /// "From: member" needs someone to choose from: basic groups and
    /// supergroups (not channels, private chats or Saved Messages).
    pub fn chat_search_can_pick_sender(&self) -> bool {
        let Some(chat_id) = self.chat_search.chat_id.or(self.open_chat) else {
            return false;
        };
        self.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.kind,
                crate::telegram::envelope::ChatKind::BasicGroup { .. }
                    | crate::telegram::envelope::ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
            )
        })
    }

    /// Open the "From:" picker (groups only; the caller checks).
    pub fn open_from_picker(&mut self) {
        if self.chat_search.open && self.chat_search.from_picker.is_none() {
            self.chat_search.from_picker = Some(FromPicker::default());
        }
    }

    pub fn close_from_picker(&mut self) {
        self.chat_search.from_picker = None;
    }

    pub(crate) fn apply_from_members(&mut self, request: RequestId, members: &[ParsedChatMember]) {
        let Some(picker) = self
            .chat_search
            .from_picker
            .as_mut()
            .filter(|p| p.request == Some(request))
        else {
            return;
        };
        picker.request = None;
        picker.members = members.iter().map(|m| m.member_id).collect();
    }

    /// A short label for a member in the picker and the "From:" chip.
    pub fn sender_label(&self, sender: MessageSender) -> String {
        match sender {
            MessageSender::User { user_id } => self
                .user(user_id)
                .map(|u| u.display_name())
                .unwrap_or_else(|| format!("user {user_id}")),
            MessageSender::Chat { chat_id } => self
                .chats
                .get(&chat_id)
                .map(|c| c.title.clone())
                .unwrap_or_else(|| format!("chat {chat_id}")),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn first_message_id_is_server_id_one() {
        assert_eq!(super::FIRST_MESSAGE_ID.0, 1_048_576);
    }
}
