//! A message sent in the "All" view of a bot that lets users create
//! threads goes into a thread of its own (tdesktop
//! `HistoryWidget::prepareSendAction` with `Data::IsBotUserCreatesTopics`
//! and `Data::Forum::reserveNewBotTopic`): the composer asks for a new
//! thread first, and sends into it once TDLib answers.
use crate::state::*;
use crate::telegram::envelope::ServiceAction;

/// The id of the "New Thread" card's row, past every real message id.
pub const NEW_BOT_THREAD_ROW_ID: i64 = -9_000_000_002;

/// How many answers of one window wait for it to take them. A window holds
/// one waiting stage and takes its own answer at once, so only a few
/// answers of abandoned asks can ever sit beside the live one.
const KEPT_PER_WINDOW: usize = 4;

/// Answers kept for the window that asked, each under its `RequestId`.
/// Another window's answers never push one out; the answers of a window
/// that closed go with it.
#[derive(Debug, Clone, PartialEq)]
pub struct AnswerBox<T> {
    held: Vec<(AskingView, RequestId, T)>,
}

impl<T> Default for AnswerBox<T> {
    fn default() -> Self {
        Self { held: Vec::new() }
    }
}

impl<T> AnswerBox<T> {
    /// Keep `value` for the window `view` that asked `request`; `open` says
    /// whether a view still exists.
    pub fn put(
        &mut self,
        view: AskingView,
        request: RequestId,
        value: T,
        open: impl Fn(AskingView) -> bool,
    ) {
        self.held.retain(|(held, ..)| open(*held));
        self.held.push((view, request, value));
        let same = self.held.iter().filter(|(held, ..)| *held == view).count();
        if same > KEPT_PER_WINDOW
            && let Some(oldest) = self.held.iter().position(|(held, ..)| *held == view)
        {
            self.held.remove(oldest);
        }
    }

    /// The answer to `request`, once.
    pub fn take(&mut self, request: RequestId) -> Option<T> {
        let at = self.held.iter().position(|(_, asked, _)| *asked == request)?;
        Some(self.held.remove(at).2)
    }

    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

impl Session {
    /// Whether a send from the compose field of `chat_id` starts a new
    /// thread first: a bot that lets users create threads, viewed as "All"
    /// (no thread open), unless the reply target already sits in a thread
    /// (that thread takes the message).
    pub fn send_starts_bot_thread(&self, chat_id: ChatId, reply_to: Option<MessageId>) -> bool {
        self.open_forum_topic().is_none()
            && self
                .bot_topics(chat_id)
                .is_some_and(|bot| bot.allows_users_to_create_topics)
            && reply_to
                .and_then(|message| self.topic_of_message(chat_id, message))
                .is_none()
    }

    /// The "New Thread" card under the "All" history of a bot that lets
    /// users create threads, once the chat has messages and no start
    /// parameter waits (tdesktop `AboutView::refresh`). It takes the date
    /// of the newest message so no date header comes before it.
    pub fn new_bot_thread_row(&self, chat_id: ChatId) -> Option<HistoryMessage> {
        if self.open_forum_topic().is_some()
            || self.bots.bot_start_params.contains_key(&chat_id.0)
            || !self
                .bot_topics(chat_id)
                .is_some_and(|bot| bot.allows_users_to_create_topics)
        {
            return None;
        }
        let newest = self
            .histories
            .get(&chat_id.0)?
            .messages
            .values()
            .next_back()?;
        Some(crate::state::session_similar::synthetic_service_row(
            chat_id,
            NEW_BOT_THREAD_ROW_ID,
            newest.date,
            ServiceAction::NewBotThread,
        ))
    }

    /// The answer to the new-thread request `request`, once it landed.
    pub fn take_bot_thread_start(&mut self, request: RequestId) -> Option<Result<i32, String>> {
        self.threads.bot_thread_starts.take(request)
    }

    /// How the topic action `request` ended, once it did.
    pub fn take_topic_action_answer(&mut self, request: RequestId) -> Option<bool> {
        self.threads.topic_action_answers.take(request)
    }

    /// Whether the window `view` is still there to take an answer.
    fn view_is_open(&self, view: AskingView) -> bool {
        match view {
            AskingView::Main => true,
            AskingView::Window { chat, opening } => {
                self.window_chats.contains(&chat) && self.window_chats.opening_of(chat) == opening
            }
        }
    }

    /// Keep how the topic action `request` of `view` ended for its window.
    pub(crate) fn topic_action_answered(&mut self, request: RequestId, view: AskingView, ok: bool) {
        let mut answers = std::mem::take(&mut self.threads.topic_action_answers);
        answers.put(view, request, ok, |view| self.view_is_open(view));
        self.threads.topic_action_answers = answers;
    }

    /// Keep the answer to the new-thread request `request` of `view` for
    /// its window.
    pub(crate) fn bot_thread_answered(
        &mut self,
        request: RequestId,
        view: AskingView,
        outcome: Result<i32, String>,
    ) {
        let mut starts = std::mem::take(&mut self.threads.bot_thread_starts);
        starts.put(view, request, outcome, |view| self.view_is_open(view));
        self.threads.bot_thread_starts = starts;
    }
}
