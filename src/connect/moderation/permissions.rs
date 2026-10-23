//! Connect driver: saving the group permissions box (tdesktop
//! `ShowEditPermissions`): the default permissions, slow mode and the
//! boosters setting, each sent only when it changed. A basic group with
//! slow mode or boosters set is upgraded first, as tdesktop migrates it.
use super::*;
use crate::permissions_box::PermissionsSave;
use crate::state::AdminFollowup;

impl<S: JsonSender> ConnectDriver<S> {
    /// Send what changed. `Ok(true)` when something was sent (or the
    /// upgrade that carries it), `Ok(false)` when nothing differs or the
    /// viewer may not save.
    pub fn save_permissions_box(
        &mut self,
        chat_id: ChatId,
        save: PermissionsSave,
    ) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let basic = matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::BasicGroup { .. })
        );
        if basic && save.needs_supergroup() {
            let followup = AdminFollowup::PermissionsAfterUpgrade { old: chat_id, save };
            return Ok(self.upgrade_with(chat_id, Some(followup))?.is_some());
        }
        self.send_permissions_save(chat_id, save)
    }

    /// The upgrade finished: save into the new supergroup.
    pub(crate) fn send_permissions_save(
        &mut self,
        chat_id: ChatId,
        save: PermissionsSave,
    ) -> Result<bool, ConnectSendError> {
        let mut sent = false;
        let current = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.permissions);
        if current != Some(save.permissions) {
            sent |= self
                .set_chat_permissions(chat_id, &save.permissions)?
                .is_some();
        }
        let is_supergroup = self.session.chat_supergroup(chat_id).is_some();
        if is_supergroup && self.session.chat_can_restrict_members(chat_id) {
            if self.session.chat_slow_mode_delay(chat_id) != save.slow_mode_delay {
                self.set_chat_slow_mode_delay(chat_id, save.slow_mode_delay)?;
                sent = true;
            }
            if self.session.chat_unrestrict_boost_count(chat_id) != save.unrestrict_boost_count {
                sent |= self
                    .set_unrestrict_boost_count(chat_id, save.unrestrict_boost_count)?
                    .is_some();
            }
        }
        Ok(sent)
    }
}
