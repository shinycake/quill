//! A message sent in the "All" view of a bot that lets users create
//! threads goes into a thread of its own (tdesktop
//! `HistoryWidget::prepareSendAction` with `Data::IsBotUserCreatesTopics`
//! and `Data::Forum::reserveNewBotTopic`): the composer asks for a new
//! thread first, and sends into it once TDLib answers.
use crate::state::*;
use crate::telegram::envelope::ServiceAction;

/// The id of the "New Thread" card's row, past every real message id.
pub const NEW_BOT_THREAD_ROW_ID: i64 = -9_000_000_002;

/// How many answers wait for a window to take them. A window takes its
/// own at once; one whose view went away before the answer never does.
const KEPT_ANSWERS: usize = 8;

/// The answer to a `createForumTopic` the composer sent for a bot's new
/// thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotThreadStarted {
    pub request: RequestId,
    /// The new thread's `forum_topic_id`, or the line saying why there is
    /// none.
    pub outcome: Result<i32, String>,
}

impl Session {
    /// Whether a send from the compose field of `chat_id` starts a new
    /// thread first: a bot that lets users create threads, viewed as "All"
    /// (no thread open), unless the reply target already sits in a thread
    /// (that thread takes the message).
    pub fn send_starts_bot_thread(&self, chat_id: ChatId, reply_to: Option<MessageId>) -> bool {
        self.open_topic.is_none()
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
        if self.open_topic.is_some()
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
        let at = self
            .threads
            .bot_thread_starts
            .iter()
            .position(|start| start.request == request)?;
        Some(self.threads.bot_thread_starts.remove(at).outcome)
    }

    /// Keep the answer to the new-thread request `request` for its window.
    pub(crate) fn bot_thread_answered(&mut self, request: RequestId, outcome: Result<i32, String>) {
        let starts = &mut self.threads.bot_thread_starts;
        starts.push(BotThreadStarted { request, outcome });
        let over = starts.len().saturating_sub(KEPT_ANSWERS);
        starts.drain(..over);
    }
}
