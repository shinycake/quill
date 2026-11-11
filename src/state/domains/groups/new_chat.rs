//! New Group / New Channel's share of the session: each create request,
//! the photo set once the chat exists, and how the request ended
//! (`crate::new_chat`).
use crate::new_chat::{CreationOutcome, NewChatStage, StageFacts};
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
    /// Out, or answered without a refusal.
    Sent,
    /// Refused or lost: the step stays, with TDLib's reason.
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewChatState {
    pub creations: HashMap<RequestId, NewChatCreation>,
    /// The requests of the type step's latest Save (a new Save replaces
    /// them), by request id: any refusal, a lost request included, keeps
    /// the step from moving on as if it saved.
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
    /// What the New Group / New Channel dialog needs to know about the
    /// requests of `stage`.
    pub fn new_chat_facts(&self, stage: &NewChatStage) -> StageFacts {
        match stage {
            NewChatStage::Creating { request, .. } => StageFacts {
                create_pending: self.requests.get(*request).is_some(),
                outcome: self
                    .groups
                    .new_chats
                    .creations
                    .get(request)
                    .and_then(|creation| creation.outcome.clone()),
                ..StageFacts::default()
            },
            NewChatStage::Type {
                chat,
                saving: Some(sent),
            } => StageFacts {
                type_pending: sent.iter().any(|id| self.requests.get(*id).is_some()),
                type_failed: self
                    .groups
                    .info_edit_errors
                    .get(&chat.0)
                    .is_some_and(|error| error.field == GroupInfoField::Username)
                    || self.groups.new_chats.type_save_failure(sent).is_some(),
                ..StageFacts::default()
            },
            _ => StageFacts::default(),
        }
    }
}
