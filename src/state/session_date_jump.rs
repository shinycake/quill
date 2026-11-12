//! Find in history: the calendar box, date jumps and the "From:" picker.
use super::*;
use crate::search_filters::SearchMediaKind;
use crate::telegram::envelope::{CalendarDay, ParsedChatMember};
use std::collections::HashMap;

/// The jump lookup of each window (a date pick, a corner "@" / heart
/// button), one per window (`None` is the main window's). A newer ask of a
/// window replaces its earlier lookup, so the earlier answer, arriving
/// after, changes nothing.
#[derive(Debug, Default)]
pub struct JumpLookups(HashMap<Option<ChatId>, JumpLookup>);

impl JumpLookups {
    fn begin(&mut self, window: Option<ChatId>, request: RequestId, chat: ChatId) {
        self.0.insert(window, JumpLookup::Asking { request, chat });
    }

    /// `request` answered or failed: whether it is the ask of its window
    /// (and so ends it).
    fn settle(&mut self, pending: Option<&PendingRequest>) -> bool {
        let Some(pending) = pending else {
            return false;
        };
        let window = pending.asked_in.chat();
        match self.0.get(&window) {
            Some(JumpLookup::Asking { request, .. }) if *request == pending.id => {
                self.0.remove(&window);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn set(&mut self, window: Option<ChatId>, lookup: JumpLookup) {
        self.0.insert(window, lookup);
    }

    /// The main window left `chat` (`Some`), or every window's lookup goes
    /// (`None`): what the main window asked, resolved or found nothing for
    /// in that chat. A chat window's lookup is its own and stays.
    fn forget_chat(&mut self, chat: Option<ChatId>) {
        let Some(chat) = chat else {
            self.0.clear();
            return;
        };
        self.0.retain(|window, lookup| match lookup {
            JumpLookup::NoMedia => window.is_some(),
            JumpLookup::Asking { chat: asked, .. } => !(window.is_none() && *asked == chat),
            JumpLookup::Resolved(jump) => !(window.is_none() && jump.chat_id == chat),
        });
    }

    /// A chat window closed: its lookup goes with it.
    pub(crate) fn forget_window(&mut self, window: ChatId) {
        self.0.remove(&Some(window));
    }

    /// The jumps resolved and waiting for the driver to start them. Notes
    /// and asks stay.
    pub(crate) fn take_resolved(&mut self) -> Vec<ViewJump> {
        let mut jumps = Vec::new();
        self.0.retain(|_, lookup| match lookup {
            JumpLookup::Resolved(jump) => {
                jumps.push(*jump);
                false
            }
            _ => true,
        });
        jumps
    }

    /// Whether `window`'s calendar box says "no media".
    fn no_media(&self, window: Option<ChatId>) -> bool {
        matches!(self.0.get(&window), Some(JumpLookup::NoMedia))
    }

    /// `window`'s note goes; its ask or resolved jump and every other
    /// window's lookup stay.
    fn clear_note_in(&mut self, window: Option<ChatId>) {
        if self.no_media(window) {
            self.0.remove(&window);
        }
    }
}

impl SearchDomainState {
    /// The line `window`'s calendar box draws for a shared-media day with
    /// no media.
    pub fn date_jump_note(&self, window: Option<ChatId>) -> Option<String> {
        self.jump_lookups.no_media(window).then(|| {
            crate::lang::tr(crate::lang::keys::Q_MISC_NO_MEDIA_TO_JUMP_TO).to_string()
        })
    }

    /// `window`'s "no media" note goes; its resolved jump stays.
    pub(crate) fn clear_date_jump_note_for(&mut self, window: Option<ChatId>) {
        self.jump_lookups.clear_note_in(window);
    }
}

impl Session {
    /// Open the calendar box for the open chat. `None` when no chat is open.
    pub fn open_history_calendar(&mut self, now: i64) -> Option<&HistoryCalendar> {
        let chat_id = self.open_chat_id()?;
        let media = if self.search.chat_search.chat_id() == Some(chat_id) {
            self.search.chat_search.media()
        } else {
            SearchMediaKind::All
        };
        let generation = self.next_calendar_generation();
        let window = self.view_in_place();
        self.search.clear_date_jump_note_for(window);
        self.search.history_calendar = Some(HistoryCalendar::new(chat_id, media, now, generation));
        self.search.history_calendar.as_ref()
    }

    /// The generation for a calendar being opened: one more than the last
    /// one opened, closed or not.
    pub(crate) fn next_calendar_generation(&mut self) -> u64 {
        self.search.calendar_generation = self.search.calendar_generation.saturating_add(1);
        self.search.calendar_generation
    }

    /// `getChatMessageByDate` or the corner buttons' `searchChatMessages`
    /// went out as `request`: it is the ask of the window that sent it.
    pub fn begin_jump_ask(&mut self, request: RequestId) {
        if let Some(pending) = self.requests.get(request)
            && let Some(chat) = pending.chat_id
        {
            let window = pending.asked_in.chat();
            self.search.jump_lookups.begin(window, request, chat);
        }
    }

    /// The corner button's search answered: the oldest hit is the jump.
    /// Only the ask of its window lands; an older ask answering after a
    /// newer one began, or one of a chat the window left, changes nothing.
    pub(crate) fn accept_unread_jump(
        &mut self,
        oldest: Option<MessageId>,
        pending: Option<&PendingRequest>,
    ) {
        if !self.search.jump_lookups.settle(pending) {
            return;
        }
        if let Some(jump) =
            oldest.and_then(|id| ViewJump::for_request(pending, id, DateJumpMode::Exact))
        {
            self.search
                .jump_lookups
                .set(jump.window, JumpLookup::Resolved(jump));
        }
    }

    /// The corner button's search failed: the window's ask ends.
    pub(crate) fn fail_unread_jump(&mut self, pending: Option<&PendingRequest>) {
        self.search.jump_lookups.settle(pending);
    }

    pub fn close_history_calendar(&mut self) {
        self.search.history_calendar = None;
    }

    /// The date-jump UI goes with its chat (or with the account leaving the
    /// screen): the calendar box, a resolved jump nobody started and the
    /// "nothing found" note. `chat` limits it to that chat's pieces
    /// (`None`: all of them).
    pub fn clear_date_jump_ui(&mut self, chat: Option<ChatId>) {
        self.search.jump_lookups.forget_chat(chat);
        let is_chat = |id: ChatId| chat.is_none_or(|chat| chat == id);
        if self
            .search
            .history_calendar
            .as_ref()
            .is_some_and(|calendar| is_chat(calendar.chat_id))
        {
            self.search.history_calendar = None;
        }
        // A date jump still loading becomes a plain one: it lands on the
        // message TDLib resolved, without retargeting.
        if let ChatSearchJump::Loading {
            request,
            message_id,
            then: JumpLanding::Date { chat_id, .. },
        } = self.search.chat_search.jump
            && is_chat(chat_id)
        {
            self.search.chat_search.jump = ChatSearchJump::Loading {
                request,
                message_id,
                then: JumpLanding::Message,
            };
        }
    }

    pub(crate) fn apply_message_calendar(
        &mut self,
        days: Vec<CalendarDay>,
        pending: Option<&PendingRequest>,
    ) {
        let Some(RequestPurpose::Search(SearchPurpose::GetChatMessageCalendar { generation })) =
            pending.map(|p| p.purpose)
        else {
            return;
        };
        let chat_id = pending.and_then(|p| p.chat_id);
        if let Some(calendar) = self
            .search
            .history_calendar
            .as_mut()
            .filter(|c| c.generation == generation && Some(c.chat_id) == chat_id)
        {
            calendar.accept(days);
        }
    }

    pub(crate) fn fail_message_calendar(&mut self, pending: Option<&PendingRequest>) {
        let Some(RequestPurpose::Search(SearchPurpose::GetChatMessageCalendar { generation })) =
            pending.map(|p| p.purpose)
        else {
            return;
        };
        if let Some(calendar) = self
            .search
            .history_calendar
            .as_mut()
            .filter(|c| c.generation == generation)
        {
            calendar.fail();
        }
    }

    /// `getChatMessageByDate` answered: the last message before the day.
    pub(crate) fn accept_date_message(
        &mut self,
        message_id: MessageId,
        pending: Option<&PendingRequest>,
    ) {
        if !self.search.jump_lookups.settle(pending) {
            return;
        }
        if let Some(jump) = ViewJump::for_request(pending, message_id, DateJumpMode::Next) {
            self.search
                .jump_lookups
                .set(jump.window, JumpLookup::Resolved(jump));
        }
    }

    /// `getChatMessageByDate` failed: a 404 means nothing is that old, so
    /// the jump goes to the start of the chat. Any other error is a line in
    /// the status note (the box closed when the day was picked, so nothing
    /// there could draw it).
    pub(crate) fn fail_date_jump(
        &mut self,
        not_found: bool,
        pending: Option<&PendingRequest>,
    ) {
        if !self.search.jump_lookups.settle(pending) {
            return;
        }
        let Some(pending) = pending else {
            return;
        };
        if !not_found {
            self.chats_state.chat_action_error.push(
                crate::lang::tr(crate::lang::keys::Q_MISC_COULD_NOT_JUMP_TO_THAT).to_string(),
            );
            return;
        }
        if let Some(jump) =
            ViewJump::for_request(Some(pending), FIRST_MESSAGE_ID, DateJumpMode::Oldest)
        {
            self.search
                .jump_lookups
                .set(pending.asked_in.chat(), JumpLookup::Resolved(jump));
        }
    }

    /// A chat window closed: its date lookup goes with it.
    pub(crate) fn forget_window_jump_lookup(&mut self, window: ChatId) {
        self.search.jump_lookups.forget_window(window);
    }

    /// Start a date jump: the same load-around pipeline as a search hit,
    /// then retarget by `mode`.
    pub fn begin_date_jump(
        &mut self,
        message_id: MessageId,
        mode: DateJumpMode,
    ) -> ChatSearchJumpNeed {
        let then = self
            .search
            .chat_search
            .chat_id()
            .or(self.open_chat_id())
            .filter(|_| mode != DateJumpMode::Exact)
            .map_or(JumpLanding::Message, |chat_id| JumpLanding::Date {
                chat_id,
                mode,
            });
        let need = self.begin_jump(message_id, then);
        if need == ChatSearchJumpNeed::AlreadyReady
            && let JumpLanding::Date { chat_id, mode } = then
        {
            self.resolve_date_jump(chat_id, message_id, mode);
        }
        need
    }

    /// The window around `message_id` is loaded: land on the day's first
    /// message (`Next`) or the chat's oldest loaded one (`Oldest`).
    pub(crate) fn resolve_date_jump(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        mode: DateJumpMode,
    ) {
        // The view moved to another chat since the jump began: its window
        // is not the one the jump was for.
        if self.search.chat_search.chat_id().or(self.open_chat_id()) != Some(chat_id) {
            return;
        }
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
            self.search.chat_search.jump = ChatSearchJump::Ready {
                message_id: MessageId(id),
            };
            self.search.chat_search.jump_serial += 1;
        }
    }

    /// "From: member" needs someone to choose from: basic groups and
    /// supergroups (not channels, private chats or Saved Messages).
    pub fn chat_search_can_pick_sender(&self) -> bool {
        let Some(chat_id) = self.search.chat_search.chat_id().or(self.open_chat_id()) else {
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
        if self.search.chat_search.is_open() && self.search.chat_search.from_picker().is_none() {
            self.search
                .chat_search
                .set_from_picker(Some(FromPicker::default()));
        }
    }

    pub fn close_from_picker(&mut self) {
        self.search.chat_search.set_from_picker(None);
    }

    pub(crate) fn apply_from_members(&mut self, request: RequestId, members: &[ParsedChatMember]) {
        let Some(picker) = self
            .search
            .chat_search
            .open_mut()
            .and_then(|open| open.from_picker.as_mut())
        else {
            return;
        };
        picker
            .members
            .answered(Some(request), members.iter().map(|m| m.member_id).collect());
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

    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::{AccountKey, ChatId, MessageId};
    use crate::state::{JumpLookup, RequestPurpose, Session};
    use std::sync::Arc;

    fn session() -> Session {
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        Session::new(AccountKey::primary(), sink)
    }

    fn ask(session: &mut Session) -> crate::ids::RequestId {
        ask_in(session, 11)
    }

    fn ask_in(session: &mut Session, chat: i64) -> crate::ids::RequestId {
        let id = session.request(RequestPurpose::GetChatMessageByDate, Some(ChatId(chat)));
        session.begin_jump_ask(id);
        id
    }

    fn resolved(session: &Session, window: Option<ChatId>) -> Option<MessageId> {
        match session.search.jump_lookups.0.get(&window) {
            Some(JumpLookup::Resolved(jump)) => Some(jump.message_id),
            _ => None,
        }
    }

    /// 09-001: the newest pick of a window wins, whatever order the answers
    /// arrive in.
    #[test]
    fn the_older_date_answer_never_wins_over_the_newer_pick() {
        for newer_first in [true, false] {
            let mut session = session();
            let older = ask(&mut session);
            let newer = ask(&mut session);
            let (first, second) = if newer_first {
                (newer, older)
            } else {
                (older, newer)
            };
            for (request, message) in [(first, 100), (second, 200)] {
                let pending = session.requests.take(request);
                session.accept_date_message(MessageId(message), pending.as_ref());
            }
            let expected = if newer_first { 100 } else { 200 };
            assert_eq!(resolved(&session, None), Some(MessageId(expected)));
        }
    }

    /// An answer nobody asked for, or one of a window that left, changes
    /// nothing.
    #[test]
    fn an_unasked_date_answer_is_dropped() {
        let mut session = session();
        let stray = session.request(RequestPurpose::GetChatMessageByDate, Some(ChatId(11)));
        let pending = session.requests.take(stray);
        session.accept_date_message(MessageId(5), pending.as_ref());
        assert_eq!(resolved(&session, None), None);
        let asked = ask(&mut session);
        session.clear_date_jump_ui(None);
        let pending = session.requests.take(asked);
        session.accept_date_message(MessageId(5), pending.as_ref());
        assert_eq!(resolved(&session, None), None);
    }

    /// The main window leaving a chat forgets the main window's ask for it,
    /// not the ask of that chat's own window; the late answer jumps nowhere.
    #[test]
    fn the_main_window_leaving_a_chat_forgets_its_own_ask_only() {
        let mut session = session();
        // The same chat open in its own window: ask in that window's view.
        session.requests.view = crate::state::AskingView::Window {
            chat: ChatId(11),
            opening: 0,
        };
        let window_ask = ask_in(&mut session, 11);
        session.requests.view = crate::state::AskingView::Main;
        let main_ask = ask_in(&mut session, 11);
        // The main window selects another chat.
        session.clear_date_jump_ui(Some(ChatId(11)));
        for request in [main_ask, window_ask] {
            let pending = session.requests.take(request);
            session.accept_date_message(MessageId(7), pending.as_ref());
        }
        assert_eq!(resolved(&session, None), None);
        assert_eq!(resolved(&session, Some(ChatId(11))), Some(MessageId(7)));
    }

    /// The corner buttons' answers settle by request as well: the older ask
    /// of a window answering after a newer one began does not win, and an
    /// error ends its ask.
    #[test]
    fn the_older_unread_answer_never_wins_over_the_newer_ask() {
        use crate::state::{MessagesPurpose, UnreadJumpKind};
        let purpose = RequestPurpose::Messages(MessagesPurpose::JumpToUnread {
            kind: UnreadJumpKind::Mention,
        });
        for newer_first in [true, false] {
            let mut session = session();
            let mut asks = Vec::new();
            for _ in 0..2 {
                let id = session.request(purpose, Some(ChatId(11)));
                session.begin_jump_ask(id);
                asks.push(id);
            }
            let order = if newer_first { [1, 0] } else { [0, 1] };
            for at in order {
                let pending = session.requests.take(asks[at]);
                session.accept_unread_jump(Some(MessageId(100 + at as i64)), pending.as_ref());
            }
            assert_eq!(resolved(&session, None), Some(MessageId(101)));
        }
        let mut session = session();
        let id = session.request(purpose, Some(ChatId(11)));
        session.begin_jump_ask(id);
        let pending = session.requests.take(id);
        session.fail_unread_jump(pending.as_ref());
        let late = session.request(purpose, Some(ChatId(11)));
        let pending = session.requests.take(late);
        session.accept_unread_jump(Some(MessageId(5)), pending.as_ref());
        assert_eq!(resolved(&session, None), None, "an ask nobody holds");
    }

    /// A chat window closing takes its lookup with it.
    #[test]
    fn a_closed_window_forgets_its_date_ask() {
        let mut session = session();
        session.requests.view = crate::state::AskingView::Window {
            chat: ChatId(11),
            opening: 0,
        };
        let asked = ask_in(&mut session, 11);
        session.requests.view = crate::state::AskingView::Main;
        session.forget_window_jump_lookup(ChatId(11));
        let pending = session.requests.take(asked);
        session.accept_date_message(MessageId(7), pending.as_ref());
        assert_eq!(resolved(&session, Some(ChatId(11))), None);
    }
}
