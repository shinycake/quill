//! Connect driver: the group and channel Type box (tdesktop
//! `edit_peer_type_box.cpp`): link checks, the public links limit and
//! freeing a link, and the save of the box's choice.
use super::*;
use crate::group_type::GroupTypeEdit;
use crate::state::{EditAfterUpgrade, GroupTypePurpose, PublicChatsFetch, PublicLinkRevoke};
use crate::telegram::requests::{
    check_chat_username, check_created_public_chats_limit, disable_all_supergroup_usernames,
    reorder_supergroup_active_usernames, set_supergroup_username,
    upgrade_basic_group_chat_to_supergroup_chat,
};

fn purpose(purpose: GroupTypePurpose) -> RequestPurpose {
    RequestPurpose::Groups(GroupsPurpose::GroupType(purpose))
}

impl<S: JsonSender> ConnectDriver<S> {
    /// `checkChatUsername` for the text in the link field. A basic group
    /// is checked as a chat being created (`chat_id` 0), as it becomes a
    /// supergroup before the link is set.
    pub fn check_group_username(
        &mut self,
        chat_id: ChatId,
        username: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let target = match self.session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Supergroup { .. }) => chat_id.0,
            Some(ChatKind::BasicGroup { .. }) => 0,
            _ => return Ok(None),
        };
        let state = &mut self.session.groups.group_type;
        state.check_generation = state.check_generation.wrapping_add(1);
        let generation = state.check_generation;
        state.check_pending = Some((chat_id, username.to_string()));
        let extra = self.session.request(
            purpose(GroupTypePurpose::CheckChatUsername { generation }),
            Some(chat_id),
        );
        self.send_json_request(extra, &check_chat_username(extra, ChatId(target), username))
            .map(Some)
    }

    /// `checkCreatedPublicChatsLimit`, asked when a private chat's Type box
    /// opens so Public can show the limit box straight away.
    pub fn check_public_chats_limit(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = purpose(GroupTypePurpose::CheckPublicChatsLimit);
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        self.send_json_request(extra, &check_created_public_chats_limit(extra))
            .map(Some)
    }

    /// `getCreatedPublicChats` for the limit box's "Your public
    /// communities" list (tdesktop `PublicsController::prepare`).
    pub fn load_created_public_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let state = &mut self.session.groups.group_type;
        if state.public_chats == PublicChatsFetch::Loading {
            return Ok(None);
        }
        state.public_chats = PublicChatsFetch::Loading;
        let extra = self
            .session
            .request(purpose(GroupTypePurpose::GetCreatedPublicChats), None);
        self.send_json_request(extra, &crate::earn::get_created_public_chats(extra))
            .map(Some)
    }

    /// Revoke from the limit box: the chat loses its link and every
    /// other active username (tdesktop `rowRightActionClicked`).
    pub fn revoke_public_link(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.groups.group_type.revoke.is_some() {
            return Ok(None);
        }
        let Some(ChatKind::Supergroup { supergroup_id, .. }) = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.kind.clone())
        else {
            return Ok(None);
        };
        let clear = self
            .session
            .request(RequestPurpose::SetSupergroupUsername, Some(chat_id));
        self.send_json_request(clear, &set_supergroup_username(clear, supergroup_id, ""))?;
        let disable = self.session.request(
            purpose(GroupTypePurpose::DisableAllUsernames),
            Some(chat_id),
        );
        self.send_json_request(
            disable,
            &disable_all_supergroup_usernames(disable, supergroup_id),
        )?;
        self.session.groups.group_type.revoke = Some(PublicLinkRevoke {
            chat_id,
            requests: vec![clear, disable],
            failed: false,
        });
        Ok(Some(clear))
    }

    /// The Edit box's Save of the Type box's choice. A basic group sends
    /// only the upgrade; `kind` and `info` wait for the new supergroup
    /// (tdesktop `saveUsername` migrates first).
    pub fn save_group_type(
        &mut self,
        chat_id: ChatId,
        kind: &[GroupTypeEdit],
        info: &[crate::group_info_edit::GroupInfoEdit],
    ) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if kind.first() == Some(&GroupTypeEdit::Upgrade) {
            if !self.session.chat_is_owner(chat_id) {
                return Err(ConnectSendError::InvalidRequest);
            }
            let extra = self
                .session
                .request(RequestPurpose::UpgradeBasicGroup, Some(chat_id));
            self.send_json_request(
                extra,
                &upgrade_basic_group_chat_to_supergroup_chat(extra, chat_id.0),
            )?;
            self.session.groups.group_type.after_upgrade.push((
                extra,
                EditAfterUpgrade {
                    old: chat_id,
                    info: info.to_vec(),
                    kind: kind.iter().skip(1).cloned().collect(),
                },
            ));
            return Ok(vec![extra]);
        }
        let mut sent = self.save_group_info(chat_id, info)?.text;
        sent.extend(self.send_group_type_edits(chat_id, kind)?);
        Ok(sent)
    }

    fn send_group_type_edits(
        &mut self,
        chat_id: ChatId,
        kind: &[GroupTypeEdit],
    ) -> Result<Vec<RequestId>, ConnectSendError> {
        let supergroup_id = match self.session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Supergroup { supergroup_id, .. }) => Some(*supergroup_id),
            _ => None,
        };
        let mut sent = Vec::new();
        for edit in kind {
            let request = match (edit, supergroup_id) {
                (GroupTypeEdit::Upgrade, _) => None,
                (GroupTypeEdit::NoForwards(on), _) => {
                    self.set_chat_protected_content(chat_id, *on)?
                }
                (_, None) => None,
                (GroupTypeEdit::DisableAllUsernames, Some(supergroup_id)) => {
                    let extra = self.session.request(
                        purpose(GroupTypePurpose::DisableAllUsernames),
                        Some(chat_id),
                    );
                    self.send_json_request(
                        extra,
                        &disable_all_supergroup_usernames(extra, supergroup_id),
                    )?;
                    Some(extra)
                }
                (GroupTypeEdit::ReorderUsernames(order), Some(supergroup_id)) => {
                    let extra = self
                        .session
                        .request(RequestPurpose::ReorderSupergroupUsernames, Some(chat_id));
                    self.send_json_request(
                        extra,
                        &reorder_supergroup_active_usernames(extra, supergroup_id, order),
                    )?;
                    Some(extra)
                }
                (GroupTypeEdit::Username(name), Some(_)) => {
                    self.set_supergroup_username(chat_id, name)?
                }
                (GroupTypeEdit::JoinToSend(on), Some(_)) => {
                    self.set_group_join_to_send(chat_id, *on)?
                }
                (
                    GroupTypeEdit::JoinByRequest {
                        enabled,
                        apply_to_invite_links,
                    },
                    Some(_),
                ) => self.toggle_supergroup_join_by_request(
                    chat_id,
                    *enabled,
                    *apply_to_invite_links,
                )?,
            };
            sent.extend(request);
        }
        Ok(sent)
    }

    /// Before `apply` takes the pending request: the rest of a save that
    /// waits for this upgrade, and whether it succeeded.
    pub(crate) fn capture_edit_after_upgrade(
        &mut self,
        owned: &crate::telegram::client::OwnedEnvelope,
    ) -> Option<(EditAfterUpgrade, bool)> {
        let extra = owned.envelope.extra?;
        let waiting = &mut self.session.groups.group_type.after_upgrade;
        let pos = waiting.iter().position(|(id, _)| *id == extra)?;
        let (_, edit) = waiting.remove(pos);
        let ok = !matches!(
            owned.envelope.payload,
            crate::telegram::envelope::EnvelopePayload::Error(_)
        );
        Some((edit, ok))
    }

    /// After `apply`: send the waiting save to the new supergroup.
    pub(crate) fn run_edit_after_upgrade(&mut self, edit: EditAfterUpgrade, ok: bool) {
        if !ok {
            return;
        }
        let Some(new) = self.session.upgraded_chat_for(edit.old) else {
            return;
        };
        if self.save_group_type(new, &edit.kind, &edit.info).is_err() {
            self.session.chats_state.chat_action_error =
                Some("the group was upgraded, but its link could not be saved".to_string());
        }
    }
}
