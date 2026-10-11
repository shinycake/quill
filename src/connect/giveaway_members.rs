//! Connect driver: the "Choose members" list of the giveaway form
//! (`crate::giveaway::AwardMembers`).
use super::*;
use crate::giveaway::MEMBERS_PAGE;
use crate::ids::{ChatId, RequestId};
use crate::state::{PaymentsPurpose, RequestPurpose};
use crate::telegram::requests::{
    get_supergroup_members, supergroup_members_filter_recent_json,
    supergroup_members_filter_search_json,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Start the list over for `query` (empty = recent members) and ask for
    /// its first page. An older search's answer is dropped.
    pub fn search_award_members(
        &mut self,
        chat_id: ChatId,
        query: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let generation = self
            .session
            .payments
            .giveaway
            .award_members
            .restart(chat_id.0, query.trim());
        self.send_award_page(generation)
    }

    /// Ask for the next page. `Ok(None)` = nothing more to load or a page
    /// already on its way.
    pub fn load_more_award_members(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let members = &mut self.session.payments.giveaway.award_members;
        if !members.can_load_more() {
            return Ok(None);
        }
        let generation = members.begin_page();
        self.send_award_page(generation).map(Some)
    }

    fn send_award_page(&mut self, generation: u64) -> Result<RequestId, ConnectSendError> {
        let (chat_id, query, offset) = {
            let m = &self.session.payments.giveaway.award_members;
            (m.chat_id, m.query.clone(), m.offset)
        };
        let fail = |this: &mut Self, err| {
            this.session.payments.giveaway.award_members.loading = false;
            Err(err)
        };
        let supergroup_id = match self.session.chats.get(&chat_id).map(|c| &c.kind) {
            Some(crate::telegram::envelope::ChatKind::Supergroup { supergroup_id, .. })
                if self.chats_path_active() =>
            {
                *supergroup_id
            }
            _ => return fail(self, ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GetAwardMembers { generation }),
            Some(ChatId(chat_id)),
        );
        let filter = if query.is_empty() {
            supergroup_members_filter_recent_json()
        } else {
            supergroup_members_filter_search_json(&query)
        };
        let json = get_supergroup_members(extra, supergroup_id, &filter, offset, MEMBERS_PAGE);
        match self.send_json_request(extra, &json) {
            Ok(id) => Ok(id),
            Err(err) => fail(self, err),
        }
    }
}
