//! Connect driver: New Group / New Channel (`crate::new_chat`). The
//! create request, then once TDLib answers with the chat, its photo
//! (`setChatPhoto`), a supergroup's or channel's invite link and, for a
//! group, opening it.
use super::*;
use crate::new_chat::{CreateFailure, CreationOutcome, NewChatKind, NewChatRequest};
use crate::profile_photo_sources::ChatPhotoInput;
use crate::state::{CreationStage, NewChatCreation, TypeSave};
use crate::telegram::envelope::{ChatsPayload, EnvelopePayload, GroupsPayload};
use crate::telegram::requests::{
    create_new_basic_group_chat, create_new_supergroup_chat, set_chat_photo,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// `createNewBasicGroupChat` (schema line 13327) for a group,
    /// `createNewSupergroupChat` (line 13337) otherwise. `photo` is set
    /// once the chat exists. `None` while the open dialog's create is out
    /// (one whose dialog was closed is `abandon_new_chat`, and does not
    /// count).
    pub fn create_new_chat(
        &mut self,
        request: &NewChatRequest,
        photo: Option<ChatPhotoInput>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || photo.as_ref().is_some_and(|photo| !photo.is_valid()) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = match request.kind {
            NewChatKind::Group => RequestPurpose::CreateBasicGroup,
            kind => RequestPurpose::Groups(GroupsPurpose::CreateSupergroupChannel {
                is_channel: kind == NewChatKind::Channel,
            }),
        };
        let busy = self
            .session
            .groups
            .new_chats
            .creations
            .iter()
            .any(|(id, creation)| {
                creation.is_asked()
                    && self
                        .session
                        .requests
                        .get(*id)
                        .is_some_and(|pending| pending.purpose == purpose)
            });
        if busy {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        let json = match request.kind {
            NewChatKind::Group => create_new_basic_group_chat(
                extra,
                &request.user_ids,
                &request.title,
                request.auto_delete,
            ),
            kind => create_new_supergroup_chat(
                extra,
                &request.title,
                kind == NewChatKind::Channel,
                &request.description,
                request.auto_delete,
            ),
        };
        self.send_json_request(extra, &json)?;
        self.session
            .groups
            .new_chats
            .creations
            .insert(extra, NewChatCreation::asked(request.kind, photo));
        Ok(Some(extra))
    }

    /// The type step's Save went out as `sent`: any of them failing, or
    /// being lost, keeps the step from moving on.
    pub fn watch_new_chat_type_save(&mut self, sent: &[RequestId]) {
        let saves = &mut self.session.groups.new_chats.type_saves;
        saves.clear();
        saves.extend(sent.iter().map(|id| (*id, TypeSave::Sent)));
    }

    /// The dialog no longer owns this create (it was closed, or its answer
    /// was taken into the next step). An answered create is forgotten; one
    /// still out comes anyway (TDLib cannot cancel it) but nothing opens it
    /// on top of whatever the person is doing now, and its entry goes when
    /// its answer is applied. One whose request is gone (lost) has no
    /// answer to wait for: its entry goes now.
    pub fn abandon_new_chat(&mut self, request: RequestId) {
        let still_out = self.session.requests.get(request).is_some();
        let creations = &mut self.session.groups.new_chats.creations;
        let Some(creation) = creations.get_mut(&request) else {
            return;
        };
        match creation.stage {
            CreationStage::Asked if still_out => creation.stage = CreationStage::Detached,
            _ => {
                creations.remove(&request);
            }
        }
    }

    /// Before `apply` takes the pending request: how one of our creates
    /// ended. The id comes back when the chat exists, for
    /// [`Self::run_new_chat_created`].
    pub(crate) fn capture_new_chat(
        &mut self,
        owned: &crate::telegram::client::OwnedEnvelope,
    ) -> Option<RequestId> {
        let extra = owned.envelope.extra?;
        let creation = self.session.groups.new_chats.creations.get_mut(&extra)?;
        let detached = match creation.stage {
            CreationStage::Asked => false,
            CreationStage::Detached => true,
            CreationStage::Answered(_) | CreationStage::Finishing(_) => return None,
        };
        let outcome = match &owned.envelope.payload {
            EnvelopePayload::Groups(GroupsPayload::CreatedBasicGroupChat { chat_id }) => {
                CreationOutcome::Created(ChatId(*chat_id))
            }
            EnvelopePayload::Chats(ChatsPayload::UpdateNewChat { chat_id, .. }) => {
                CreationOutcome::Created(*chat_id)
            }
            EnvelopePayload::Error(err) => CreationOutcome::Failed {
                failure: CreateFailure::from_server_type(err.server_type.as_deref()),
                reason: crate::state::error_reason(err),
            },
            _ => return None,
        };
        // Nobody reads the answer of a create whose dialog is gone: a
        // refusal is forgotten now, a made chat after its photo and link.
        match (outcome, detached) {
            (CreationOutcome::Failed { .. }, true) => {
                self.session.groups.new_chats.creations.remove(&extra);
                None
            }
            (CreationOutcome::Created(chat), true) => {
                creation.stage = CreationStage::Finishing(chat);
                Some(extra)
            }
            (outcome @ CreationOutcome::Created(_), false) => {
                creation.stage = CreationStage::Answered(outcome);
                Some(extra)
            }
            (outcome, false) => {
                creation.stage = CreationStage::Answered(outcome);
                None
            }
        }
    }

    /// After `apply`: the new chat's photo, then open a group.
    pub(crate) fn run_new_chat_created(&mut self, extra: RequestId) {
        let Some(creation) = self.session.groups.new_chats.creations.get_mut(&extra) else {
            return;
        };
        let (chat_id, dialog_reads) = match creation.stage {
            CreationStage::Answered(CreationOutcome::Created(chat)) => (chat, true),
            CreationStage::Finishing(chat) => (chat, false),
            _ => return,
        };
        let photo = creation.photo.take();
        let open = creation.kind == NewChatKind::Group && dialog_reads;
        let make_invite_link = creation.kind.is_supergroup();
        if let Some(photo) = photo {
            let photo_request = self
                .session
                .request(RequestPurpose::SetChatPhoto, Some(chat_id));
            let json = set_chat_photo(photo_request, chat_id, photo.to_json());
            if self.send_json_request(photo_request, &json).is_err() {
                self.session
                    .chats_state
                    .chat_action_error
                    .push(crate::lang::tr(crate::lang::keys::Q_NEW_CHAT_PHOTO_FAILED).to_string());
            }
        }
        if make_invite_link && self.session.chat_primary_invite_link(chat_id).is_none() {
            let _ = self.replace_primary_chat_invite_link(chat_id);
        }
        if open {
            let _ = self.select_chat(chat_id);
        }
        if !dialog_reads {
            self.session.groups.new_chats.creations.remove(&extra);
        }
    }
}
