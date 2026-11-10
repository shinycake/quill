//! What the requests a folder or contact box waits on ended in, by request
//! id. A box keeps the id of the request it sent (its `Saving(request)`
//! stage) and takes only that answer, so another box's answer, or a late
//! one for a box that was cancelled and reopened, never settles it.
use crate::state::*;
use crate::telegram::envelope::ChatListPayload;
use std::collections::VecDeque;

/// How many answers wait for their box: a box that closed never takes its
/// answer, so the oldest go first.
const KEPT: usize = 16;

/// The end of one request a box waited on.
#[derive(Debug, Clone, PartialEq)]
pub enum DialogAnswer {
    /// It went through.
    Done,
    /// `checkChatFolderInviteLink` answered with the folder the link adds.
    Checked(Box<ChatFolderInviteLinkInfo>),
    /// It failed; the line is what the box shows.
    Failed(String),
}

/// The answers of box requests not yet taken ([`DialogAnswer`]).
#[derive(Debug, Default)]
pub struct DialogAnswers {
    answers: VecDeque<(RequestId, DialogAnswer)>,
}

impl DialogAnswers {
    /// The answer of `request`, once it came. Takes it.
    pub fn take(&mut self, request: RequestId) -> Option<DialogAnswer> {
        let index = self.answers.iter().position(|(id, _)| *id == request)?;
        self.answers.remove(index).map(|(_, answer)| answer)
    }

    fn put(&mut self, request: RequestId, answer: DialogAnswer) {
        self.answers.push_back((request, answer));
        while self.answers.len() > KEPT {
            self.answers.pop_front();
        }
    }

    /// Notes the answer to `pending` when a box waits on its purpose. The
    /// one place every payload and every error passes through
    /// (`Session::apply_payload`).
    pub(crate) fn note(&mut self, pending: Option<&PendingRequest>, payload: &EnvelopePayload) {
        let Some(pending) = pending.filter(|p| p.purpose.answered_to_a_box()) else {
            return;
        };
        let answer = match payload {
            EnvelopePayload::Error(err) => DialogAnswer::Failed(error_reason(err)),
            EnvelopePayload::ChatList(ChatListPayload::ChatFolderInviteLinkInfo(info)) => {
                DialogAnswer::Checked(Box::new(info.clone()))
            }
            _ => DialogAnswer::Done,
        };
        self.put(pending.id, answer);
    }
}

impl RequestPurpose {
    /// Whether a box waits on the answer of this request: its end is kept
    /// for the box to take ([`DialogAnswers`]).
    pub(crate) fn answered_to_a_box(self) -> bool {
        matches!(
            self,
            RequestPurpose::CreateChatFolder
                | RequestPurpose::EditChatFolder
                | RequestPurpose::CreateChatFolderInviteLink
                | RequestPurpose::EditChatFolderInviteLink
                | RequestPurpose::DeleteChatFolderInviteLink
                | RequestPurpose::CheckChatFolderInviteLink
                | RequestPurpose::AddChatFolderByInviteLink
                | RequestPurpose::AddContact
        )
    }
}
