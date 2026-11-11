//! New Group / New Channel's share of the session: each create request,
//! the photo set once the chat exists, and how the request ended
//! (`crate::new_chat`).
use crate::lang::{keys, tr};
use crate::new_chat::{CreateProgress, CreationOutcome, NewChatStage, TypeSaveStage};
use crate::profile_photo_sources::ChatPhotoInput;
use crate::state::*;

/// Whose create this is: the open dialog's, or one whose dialog was closed
/// while it was out (it still finishes, but opens nothing and does not
/// block the next dialog's Create).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CreationAsker {
    #[default]
    Dialog,
    Closed,
}

/// One create request.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewChatCreation {
    /// `setChatPhoto` once the chat exists.
    pub photo: Option<ChatPhotoInput>,
    /// A group opens as soon as it exists; a channel when its last step
    /// closes.
    pub open_when_created: bool,
    /// A supergroup or channel gets its primary invite link straight
    /// away, for the type step (tdesktop `checkInviteLink`).
    pub make_invite_link: bool,
    pub outcome: Option<CreationOutcome>,
    pub asker: CreationAsker,
}

/// A request of the type step's Save, from the send to its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeSave {
    /// Out: its answer has not come.
    Sent,
    /// Answered without a refusal.
    Done,
    /// Refused or lost: the step stays, with TDLib's reason.
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewChatState {
    pub creations: HashMap<RequestId, NewChatCreation>,
    /// The requests of the type step's latest Save (a new Save replaces
    /// them), by request id: any refusal keeps the step from moving on as
    /// if it saved. A request that is gone while still `Sent` was lost.
    pub type_saves: HashMap<RequestId, TypeSave>,
}

impl NewChatState {
    /// The reason one request of the type step's Save failed.
    pub fn type_save_failure(&self, sent: &[RequestId]) -> Option<&str> {
        sent.iter().find_map(|id| match self.type_saves.get(id) {
            Some(TypeSave::Failed(reason)) => Some(reason.as_str()),
            _ => None,
        })
    }
}

impl Session {
    /// `stage` after the session's answers to its requests: a create that
    /// was answered or lost, a type Save whose requests ended (all well:
    /// the subscribers step; any refused or lost: the step with TDLib's
    /// reason). A stage with nothing to settle comes back as it was.
    pub fn settled_new_chat(&self, stage: &NewChatStage) -> NewChatStage {
        match stage {
            NewChatStage::Creating {
                request,
                back,
                progress: CreateProgress::InFlight,
            } => {
                let outcome = self
                    .groups
                    .new_chats
                    .creations
                    .get(request)
                    .and_then(|creation| creation.outcome.clone());
                let progress = match outcome {
                    Some(CreationOutcome::Created(chat)) => CreateProgress::Created(chat),
                    Some(CreationOutcome::Failed { failure, reason }) => {
                        CreateProgress::Refused { failure, reason }
                    }
                    None if self.requests.get(*request).is_some() => CreateProgress::InFlight,
                    None => CreateProgress::Lost,
                };
                NewChatStage::Creating {
                    request: *request,
                    back: *back,
                    progress,
                }
            }
            NewChatStage::Type {
                chat,
                save: TypeSaveStage::Saving(sent),
            } => {
                if sent.iter().any(|id| self.requests.get(*id).is_some()) {
                    return stage.clone();
                }
                let saves = &self.groups.new_chats.type_saves;
                let refusal = sent.iter().find_map(|id| match saves.get(id) {
                    Some(TypeSave::Done) => None,
                    Some(TypeSave::Failed(reason)) => Some(reason.clone()),
                    // Gone without an answer: the Save did not happen.
                    Some(TypeSave::Sent) | None => {
                        Some(tr(keys::Q_MISC_COULDNT_REACH_TELEGRAM_TRY_AGAIN).to_string())
                    }
                });
                match refusal {
                    Some(reason) => NewChatStage::Type {
                        chat: *chat,
                        save: TypeSaveStage::Refused(reason),
                    },
                    None => NewChatStage::Subscribers { chat: *chat },
                }
            }
            other => other.clone(),
        }
    }
}
