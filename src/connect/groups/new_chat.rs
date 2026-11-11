//! Connect driver: New Group / New Channel (`crate::new_chat`). The
//! create request, then once TDLib answers with the chat, its photo
//! (`setChatPhoto`), a supergroup's or channel's invite link and, for a
//! group, opening it.
use super::*;
use crate::new_chat::{CreateFailure, CreationOutcome, NewChatKind, NewChatRequest};
use crate::profile_photo_sources::ChatPhotoInput;
use crate::state::{CreationAsker, NewChatCreation, TypeSave};
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
                creation.asker == CreationAsker::Dialog
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
        self.session.groups.new_chats.creations.insert(
            extra,
            NewChatCreation {
                photo,
                open_when_created: request.kind == NewChatKind::Group,
                make_invite_link: request.kind.is_supergroup(),
                outcome: None,
                asker: CreationAsker::Dialog,
            },
        );
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
    /// its answer is applied.
    pub fn abandon_new_chat(&mut self, request: RequestId) {
        let creations = &mut self.session.groups.new_chats.creations;
        match creations.get_mut(&request) {
            Some(creation) if creation.outcome.is_none() => {
                creation.asker = CreationAsker::Closed;
            }
            Some(_) => {
                creations.remove(&request);
            }
            None => {}
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
        if creation.outcome.is_some() {
            return None;
        }
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
        let created = matches!(outcome, CreationOutcome::Created(_));
        // Nobody reads the answer of a create whose dialog is gone: a
        // refusal is forgotten now, a made chat after its photo and link.
        if !created && creation.asker == CreationAsker::Closed {
            self.session.groups.new_chats.creations.remove(&extra);
            return None;
        }
        creation.outcome = Some(outcome);
        created.then_some(extra)
    }

    /// After `apply`: the new chat's photo, then open a group.
    pub(crate) fn run_new_chat_created(&mut self, extra: RequestId) {
        let Some(creation) = self.session.groups.new_chats.creations.get_mut(&extra) else {
            return;
        };
        let Some(CreationOutcome::Created(chat_id)) = creation.outcome else {
            return;
        };
        let photo = creation.photo.take();
        let dialog_reads = creation.asker == CreationAsker::Dialog;
        let open = creation.open_when_created && dialog_reads;
        let make_invite_link = creation.make_invite_link;
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
