//! Connect driver: polls.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::poll::{PollDraft, can_stop_poll, poll_answer_for_tap};
use crate::state::{PollVotersFetch, RequestPurpose};
use crate::telegram::envelope::MessageContent;
use crate::telegram::requests::{
    PollSend, PollTypeSend, SendReply, get_poll_voters, send_poll, set_poll_answer,
    stop_poll as stop_poll_request,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase 4.2: `setPollAnswer` for a poll-row tap. Guards mirror the other
    /// send methods: chats path active, supported chat, real non-pending
    /// message whose content is a votable `messagePoll`. The tap resolves to
    /// the full answer set via `poll_answer_for_tap` (tdesktop-style
    /// toggle/retract semantics); `None` there means no-op (closed poll,
    /// unchanged answer) and no request goes out. On send, the chosen marks
    /// flip locally right away; the server's `updatePoll` corrects the
    /// counts/percentages in place.
    pub fn send_poll_answer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let poll = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .filter(|message| !message.pending && message.id.0 > 0)
            .and_then(|message| match &message.content {
                MessageContent::Poll(poll) => Some(poll.poll.clone()),
                _ => None,
            })
            .ok_or(ConnectSendError::InvalidRequest)?;
        if !poll.can_vote() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let option_ids =
            poll_answer_for_tap(&poll, option_index).ok_or(ConnectSendError::InvalidRequest)?;
        // Capture the previous chosen marks: the optimistic flip below is
        // rolled back if the send fails (the server's `updatePoll` corrects
        // counts/percentages in place on success).
        let mut previous: Vec<bool> = Vec::new();
        if let Some(history) = self.session.histories.get_mut(&chat_id.0)
            && let Some(message) = history.messages.get_mut(&message_id.0)
            && let MessageContent::Poll(poll_content) = &mut message.content
        {
            let chosen: std::collections::HashSet<i32> = option_ids.iter().copied().collect();
            for (index, option) in poll_content.poll.options.iter_mut().enumerate() {
                previous.push(option.is_chosen);
                option.is_chosen = chosen.contains(&(index as i32));
            }
        }
        let extra = self
            .session
            .request(RequestPurpose::SetPollAnswer, Some(chat_id));
        let json = set_poll_answer(extra, chat_id, message_id, &option_ids);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                // The vote never left the client: restore the previous marks.
                if let Some(history) = self.session.histories.get_mut(&chat_id.0)
                    && let Some(message) = history.messages.get_mut(&message_id.0)
                    && let MessageContent::Poll(poll_content) = &mut message.content
                {
                    for (option, &was_chosen) in
                        poll_content.poll.options.iter_mut().zip(previous.iter())
                    {
                        option.is_chosen = was_chosen;
                    }
                }
                Err(err)
            }
        }
    }

    /// B4: one `getPollVoters` page (`schema/td_api.tl:12941`; page size 50,
    /// the schema max). Guards mirror `send_poll_answer` plus the
    /// `poll.can_get_voters` gate (schema line 12941). `option_index` is
    /// the 0-based option index. The answer lands in
    /// `Session::poll_voters` (first page replaces, later pages append).
    pub fn fetch_poll_voters(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.fetch_poll_voters_page(chat_id, message_id, option_index, 0)
    }

    /// B4: the next `getPollVoters` page — `offset` is the already-loaded
    /// count. No-op unless the cache holds a loaded page; exhaustion is
    /// handled in the reducer, which clamps `total_count` on a short
    /// page so the UI hides "Load more".
    pub fn load_more_poll_voters(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let key = (chat_id.0, message_id.0, option_index as i32);
        let offset = match self.session.poll_voters.get(&key) {
            Some(PollVotersFetch::Loaded { voters, .. }) => voters.len() as i32,
            _ => return Ok(None),
        };
        self.fetch_poll_voters_page(chat_id, message_id, option_index, offset)
    }

    /// B4: page size for `getPollVoters` (schema line 12941: limit ≤ 50).
    pub const POLL_VOTERS_PAGE_SIZE: i32 = 50;

    fn fetch_poll_voters_page(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
        offset: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let poll = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .filter(|message| !message.pending && message.id.0 > 0)
            .and_then(|message| match &message.content {
                MessageContent::Poll(poll) => Some(poll.poll.clone()),
                _ => None,
            })
            .ok_or(ConnectSendError::InvalidRequest)?;
        if !poll.can_get_voters || option_index >= poll.options.len() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let key = (chat_id.0, message_id.0, option_index as i32);
        if matches!(
            self.session.poll_voters.get(&key),
            Some(PollVotersFetch::Loading)
        ) || self.session.requests.has_purpose_for_chat(
            RequestPurpose::GetPollVoters {
                chat_id,
                message_id,
                option_id: option_index as i32,
                offset,
            },
            chat_id,
        ) {
            return Ok(None);
        }
        // A "load more" must not clobber the loaded page while it flies.
        if offset > 0
            && !matches!(
                self.session.poll_voters.get(&key),
                Some(PollVotersFetch::Loaded { .. })
            )
        {
            return Ok(None);
        }
        if offset == 0 {
            self.session
                .poll_voters
                .insert(key, PollVotersFetch::Loading);
        }
        let extra = self.session.request(
            RequestPurpose::GetPollVoters {
                chat_id,
                message_id,
                option_id: option_index as i32,
                offset,
            },
            Some(chat_id),
        );
        let json = get_poll_voters(
            extra,
            chat_id,
            message_id,
            option_index as i32,
            offset,
            Self::POLL_VOTERS_PAGE_SIZE,
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                if offset == 0 {
                    self.session.poll_voters.remove(&key);
                }
                Err(err)
            }
        }
    }

    /// B4: stop a poll / quiz via `stopPoll` (schema 1.8.67, line 12953).
    /// Guards: chats path active, supported chat, the message is a live
    /// open poll. The UI confirms before calling; `can_be_edited`
    /// (schema line 12951) is the server gate and the UI only offers it
    /// on own polls (`poll::can_stop_poll`). Response is `ok`; the poll
    /// closes via `updatePoll`.
    pub fn stop_poll(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_stop = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .filter(|message| !message.pending && message.id.0 > 0)
            .and_then(|message| match &message.content {
                // F4: defense in depth — the UI menu already gates on
                // `can_stop_poll`, but the driver checks ownership too.
                MessageContent::Poll(poll) => Some(can_stop_poll(message.is_outgoing, &poll.poll)),
                _ => None,
            })
            .unwrap_or(false);
        if !can_stop {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::StopPoll, Some(chat_id));
        let json = stop_poll_request(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 4.2: create a poll from the composer dialog via `sendMessage` +
    /// `inputMessagePoll` (TDLib 1.8.67). Guards: chats path active, chat can
    /// post (admin-gated channels, same as `send_snapshot`), valid draft
    /// (`PollDraft::validate`). Quiz drafts send `inputPollTypeQuiz`
    /// (schema line 488); quiz mode forces no revoting (Telegram X
    /// `CreatePollController` does the same on quiz toggle) and
    /// single-answer (Quill's own stricter choice; TGX allows
    /// multi-correct quizzes).
    pub fn send_poll_draft(
        &mut self,
        chat_id: ChatId,
        draft: &PollDraft,
        reply_to: Option<SendReply>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if draft.validate().is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Parity slice 4: never send into a closed forum topic (the
        // composer is hidden there; this guards a stale-snapshot race).
        if self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let question = draft.question.trim().to_string();
        let description = draft.description.trim().to_string();
        let options: Vec<String> = draft
            .usable_options()
            .into_iter()
            .map(str::to_string)
            .collect();
        let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
        let country_refs: Vec<&str> = draft.country_codes.iter().map(String::as_str).collect();
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        // Parity slice 4: sends from a topic view address the open topic.
        let topic_id = self.send_topic(chat_id);
        // `validate()` above guarantees `quiz_correct` is `Some` in quiz mode.
        let correct_option_ids: Vec<i32> = vec![draft.quiz_correct.unwrap_or(0) as i32];
        let json = send_poll(
            extra,
            chat_id,
            PollSend {
                question: &question,
                options: &option_refs,
                description: &description,
                is_anonymous: draft.is_anonymous,
                allows_multiple_answers: draft.allows_multiple_answers && !draft.is_quiz,
                allows_revoting: draft.allows_revoting && !draft.is_quiz,
                shuffle_options: draft.shuffle_options,
                country_codes: &country_refs,
                poll_type: if draft.is_quiz {
                    PollTypeSend::Quiz {
                        correct_option_ids: &correct_option_ids,
                        explanation: draft.quiz_explanation.trim(),
                    }
                } else {
                    PollTypeSend::Regular
                },
                open_period: draft.open_period_secs(),
                reply_to,
                topic_id,
            },
        );
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
