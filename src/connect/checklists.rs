//! Connect driver: checklists and poll extras (B15).
use super::*;
use crate::checklist::{ChecklistDraft, next_task_ids, toggle_task_request, validate_added_tasks};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::poll::{can_offer_add_option, validate_new_option};
use crate::state::{PollStatsFetch, RequestPurpose};
use crate::telegram::envelope::{Checklist, MessageContent};
use crate::telegram::requests::{
    ChecklistSend, SendReply, add_checklist_tasks, add_poll_option, get_poll_vote_statistics,
    mark_checklist_tasks_as_done, send_checklist,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Guard shared by the B15 message mutations: chats path active, a
    /// supported chat, and a real (sent) message.
    fn sent_message_content(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<&MessageContent, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .filter(|message| !message.pending && message.id.0 > 0)
            .map(|message| &message.content)
            .ok_or(ConnectSendError::InvalidRequest)
    }

    fn sent_checklist(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Checklist, ConnectSendError> {
        match self.sent_message_content(chat_id, message_id)? {
            MessageContent::Checklist(content) => Ok(content.list.clone()),
            _ => Err(ConnectSendError::InvalidRequest),
        }
    }

    fn send_tracked(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.session.request(purpose, Some(chat_id));
        let json = build(extra);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `addPollOption` (schema 1.8.67, line 12920) from the "Add an Option"
    /// row. Guards: the message is a live poll whose `can_add_option` is
    /// set, the poll is open, and the text passes `validate_new_option`
    /// (non-empty, ≤ 100 chars, not a duplicate).
    pub fn add_poll_option(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let text = match self.sent_message_content(chat_id, message_id)? {
            MessageContent::Poll(content)
                if can_offer_add_option(content.can_add_option, &content.poll) =>
            {
                validate_new_option(&content.poll, text)
                    .map_err(|_| ConnectSendError::InvalidRequest)?
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        self.send_tracked(RequestPurpose::AddPollOption, chat_id, |extra| {
            add_poll_option(extra, chat_id, message_id, &text)
        })
    }

    /// `getPollVoteStatistics` (schema 1.8.67, line 12947). The caller
    /// gates on `messageProperties.can_get_poll_vote_statistics`. The
    /// answer lands in `Session::poll_stats`.
    pub fn fetch_poll_vote_statistics(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !matches!(
            self.sent_message_content(chat_id, message_id)?,
            MessageContent::Poll(_)
        ) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let key = (chat_id.0, message_id.0);
        if matches!(
            self.session.poll_stats.get(&key),
            Some(PollStatsFetch::Loading)
        ) {
            return Ok(None);
        }
        self.session.poll_stats.insert(key, PollStatsFetch::Loading);
        let result = self.send_tracked(
            RequestPurpose::GetPollVoteStatistics {
                chat_id,
                message_id,
            },
            chat_id,
            |extra| get_poll_vote_statistics(extra, chat_id, message_id, is_dark),
        );
        match result {
            Ok(extra) => Ok(Some(extra)),
            Err(err) => {
                self.session.poll_stats.remove(&key);
                Err(err)
            }
        }
    }

    /// `markChecklistTasksAsDone` (schema 1.8.67, line 12967): tapping a
    /// task flips it (done -> not done and back). Premium only
    /// (`messageProperties.can_mark_tasks_as_done`); `None` from
    /// `toggle_task_request` (no permission / unknown task) is a no-op
    /// error.
    pub fn toggle_checklist_task(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        task_id: i32,
    ) -> Result<RequestId, ConnectSendError> {
        let list = self.sent_checklist(chat_id, message_id)?;
        if !self.session.my_is_premium() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (done, not_done) =
            toggle_task_request(&list, task_id).ok_or(ConnectSendError::InvalidRequest)?;
        self.send_tracked(RequestPurpose::MarkChecklistTasks, chat_id, |extra| {
            mark_checklist_tasks_as_done(extra, chat_id, message_id, &done, &not_done)
        })
    }

    /// `addChecklistTasks` (schema 1.8.67, line 12960) from the "Add Tasks"
    /// box. Premium only; `Checklist::can_add_tasks` and the 30-task limit
    /// are enforced here.
    pub fn add_checklist_tasks(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        texts: &[String],
    ) -> Result<RequestId, ConnectSendError> {
        let list = self.sent_checklist(chat_id, message_id)?;
        if !self.session.my_is_premium() || !list.can_add_tasks {
            return Err(ConnectSendError::InvalidRequest);
        }
        let tasks =
            validate_added_tasks(&list, texts).map_err(|_| ConnectSendError::InvalidRequest)?;
        let ids = next_task_ids(&list, tasks.len());
        let pairs: Vec<(i32, &str)> = ids
            .iter()
            .copied()
            .zip(tasks.iter().map(String::as_str))
            .collect();
        self.send_tracked(RequestPurpose::AddChecklistTasks, chat_id, |extra| {
            add_checklist_tasks(extra, chat_id, message_id, &pairs)
        })
    }

    /// Create a checklist from the composer dialog via `sendMessage` +
    /// `inputMessageChecklist` (schema 1.8.67, line 6209). Guards: chats
    /// path active, Premium (schema: "for Telegram Premium users only"),
    /// `can_create_checklist`, valid draft, not a closed forum topic.
    pub fn send_checklist_draft(
        &mut self,
        chat_id: ChatId,
        draft: &ChecklistDraft,
        reply_to: Option<SendReply>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || draft.validate().is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let allowed = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            chat.can_post() && crate::checklist::can_create_checklist(chat, true)
        });
        if !allowed || !self.session.my_is_premium() || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let title = draft.title.trim().to_string();
        let tasks = draft.usable_tasks();
        let topic_id = self.send_topic(chat_id);
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let json = send_checklist(
            extra,
            chat_id,
            ChecklistSend {
                title: &title,
                tasks: &tasks,
                others_can_add_tasks: draft.others_can_add_tasks,
                others_can_mark_tasks_as_done: draft.others_can_mark_tasks_as_done,
                reply_to,
                topic_id,
            },
        );
        let json = self.thread_routed(chat_id, json);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
