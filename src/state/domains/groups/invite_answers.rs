//! Answers of the invite-link list requests: each lands only in the slot
//! that still waits for its own request.
use crate::state::*;

impl Session {
    /// A `chatInviteLinks` answer (`getChatInviteLinks`,
    /// `getRevokedChatInviteLinks`, `getAdminChatInviteLinks`,
    /// `revokeChatInviteLink`).
    pub(super) fn apply_chat_invite_links(
        &mut self,
        total_count: i32,
        links: Vec<ParsedChatInviteLink>,
        pending: Option<&PendingRequest>,
    ) {
        let Some(pending) = pending else {
            return;
        };
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let loaded = InviteLinkFetch::Loaded(InviteLinkList { total_count, links });
        match pending.purpose {
            // An older list, answered after a change or a refresh asked
            // again, never covers the newer one.
            RequestPurpose::GetChatInviteLinks => {
                if let Some(slot) = self.groups.invite_links.get_mut(&chat_id.0)
                    && *slot == InviteLinkFetch::Loading(pending.id)
                {
                    *slot = loaded;
                }
            }
            RequestPurpose::GetRevokedChatInviteLinks => {
                if let Some(slot) = self.groups.revoked_invite_links.get_mut(&chat_id.0)
                    && *slot == InviteLinkFetch::Loading(pending.id)
                {
                    *slot = loaded;
                }
            }
            // B8: the revoke answer holds only the revoked link (and the
            // replacement primary), not the whole list.
            RequestPurpose::RevokeChatInviteLink => {
                let InviteLinkFetch::Loaded(list) = loaded else {
                    return;
                };
                self.apply_revoke_answer(chat_id.0, list.links);
            }
            RequestPurpose::GetAdminChatInviteLinks { revoked } => {
                if let Some(state) = self.groups.invite_pages.admin_links_mut(chat_id.0) {
                    let slot = if revoked {
                        &mut state.revoked
                    } else {
                        &mut state.active
                    };
                    if *slot == InviteLinkFetch::Loading(pending.id) {
                        *slot = loaded;
                    }
                }
            }
            _ => {}
        }
    }
}
