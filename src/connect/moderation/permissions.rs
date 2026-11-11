//! Connect driver: saving the group permissions box (tdesktop
//! `ShowEditPermissions`): the default permissions, slow mode and the
//! boosters setting, each sent only when it changed. A basic group with
//! slow mode or boosters set is upgraded first, as tdesktop migrates it.
use super::*;
use crate::permissions_box::{PermissionsPart, PermissionsSave, PermissionsSent};
use crate::state::AdminFollowup;

impl<S: JsonSender> ConnectDriver<S> {
    /// Send what changed.
    pub fn save_permissions_box(
        &mut self,
        chat_id: ChatId,
        save: PermissionsSave,
    ) -> Result<PermissionsSent, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let basic = matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::BasicGroup { .. })
        );
        if basic && save.needs_supergroup() {
            let followup = AdminFollowup::PermissionsAfterUpgrade { old: chat_id, save };
            return Ok(match self.upgrade_with(chat_id, Some(followup))? {
                Some(id) => PermissionsSent::Sent(vec![id]),
                None => PermissionsSent::Refused,
            });
        }
        self.send_permissions_save(chat_id, save)
    }

    /// The upgrade finished: save into the new supergroup.
    pub(crate) fn send_permissions_save(
        &mut self,
        chat_id: ChatId,
        save: PermissionsSave,
    ) -> Result<PermissionsSent, ConnectSendError> {
        let mut sent = Vec::new();
        let mut differs = false;
        let current = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.permissions);
        if current != Some(save.permissions) {
            differs = true;
            sent.extend(self.set_chat_permissions(chat_id, &save.permissions)?);
        }
        let is_supergroup = self.session.chat_supergroup(chat_id).is_some();
        if is_supergroup && self.session.chat_can_restrict_members(chat_id) {
            if self.session.chat_slow_mode_delay(chat_id) != save.slow_mode_delay {
                differs = true;
                match self.set_chat_slow_mode_delay(chat_id, save.slow_mode_delay) {
                    Ok(id) => sent.push(id),
                    Err(err) => return partial_or(sent, PermissionsPart::SlowMode, err),
                }
            }
            if self.session.chat_unrestrict_boost_count(chat_id) != save.unrestrict_boost_count {
                differs = true;
                match self.set_unrestrict_boost_count(chat_id, save.unrestrict_boost_count) {
                    Ok(id) => sent.extend(id),
                    Err(err) => return partial_or(sent, PermissionsPart::Boosts, err),
                }
            }
        }
        Ok(match (sent.is_empty(), differs) {
            (false, _) => PermissionsSent::Sent(sent),
            (true, true) => PermissionsSent::Refused,
            (true, false) => PermissionsSent::Unchanged,
        })
    }
}

/// A later part could not be sent: with earlier parts out the Save is
/// partial and says which part, with none out it is the plain error.
fn partial_or(
    sent: Vec<RequestId>,
    refused: PermissionsPart,
    err: ConnectSendError,
) -> Result<PermissionsSent, ConnectSendError> {
    if sent.is_empty() {
        Err(err)
    } else {
        Ok(PermissionsSent::Partial { sent, refused })
    }
}
