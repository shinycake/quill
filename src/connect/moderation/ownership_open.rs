//! What the transfer-ownership and owner-leave boxes ask when they open, and
//! on their Retry: a request that cannot be sent lands in a `Failed` stage the
//! box draws with Retry, never in the "still checking" one.
use super::*;
use crate::state::send_failed_line;

const SEND_FAILED: &str = "Could not check whether the owner can be changed";

impl<S: JsonSender> ConnectDriver<S> {
    /// The box opened for `chat_id`: the gate, the heir after leaving and
    /// the member lists the new owner is picked from.
    pub fn open_ownership(&mut self, chat_id: ChatId) {
        self.session.groups.ownership.transfer.forget_settled();
        // Asked once per dialog so a stale answer never decides.
        self.session
            .groups
            .ownership
            .owner_after_leaving
            .remove(&chat_id.0);
        let _ = self.check_can_transfer_ownership();
        let _ = self.fetch_chat_owner_after_leaving(chat_id);
        self.request_ownership_candidates(chat_id);
    }

    /// The member lists the new owner is picked from (the Pick step's
    /// Retry too). A list that cannot be asked is `Failed`.
    pub fn request_ownership_candidates(&mut self, chat_id: ChatId) {
        let failed = || SupergroupMembersFetch::Failed(send_failed_line("Could not load members"));
        for filter in [MemberListFilter::Administrators, MemberListFilter::Recent] {
            if self
                .refresh_supergroup_members(chat_id, filter, "")
                .is_err()
            {
                self.session
                    .groups
                    .set_member_fetch((chat_id.0, filter), failed());
            }
        }
        if self.fetch_basic_group_members(chat_id).is_err() {
            self.session
                .groups
                .basic_group_members
                .insert(chat_id.0, failed());
        }
    }

    /// The gate or the heir lookup never left.
    pub(super) fn fail_ownership_check_send(&mut self) {
        self.session
            .groups
            .ownership
            .check
            .fail_dropping_data(send_failed_line(SEND_FAILED));
    }

    pub(super) fn fail_owner_lookup_send(&mut self, chat_id: ChatId) {
        self.session.groups.ownership.owner_after_leaving.insert(
            chat_id.0,
            OwnerLookup::Failed(send_failed_line("Could not find out who becomes the owner")),
        );
    }
}
