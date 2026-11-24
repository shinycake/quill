//! Connect driver: `inviteUsersToBot` from the "Invite to Bot" box
//! (`crate::invite_to_bot`, TDLib 1.8.71).
use super::*;
use crate::ids::RequestId;
use crate::state::{BotsPurpose, RequestPurpose};
use crate::telegram::requests::invite_users_to_bot;

impl<S: JsonSender> ConnectDriver<S> {
    /// Whether the user owns `bot_id` (`userTypeBot.can_be_edited`), the
    /// only bots TDLib lets them invite people to.
    pub fn can_invite_to_bot(&self, bot_id: i64) -> bool {
        self.session
            .user(bot_id)
            .is_some_and(|user| user.is_bot && user.can_be_edited)
    }

    /// Sends the invite; the box's `Sending` stage holds the request, and
    /// its answer is settled in `session.acks`.
    pub fn invite_users_to_bot(
        &mut self,
        bot_id: i64,
        user_ids: &[i64],
    ) -> Result<RequestId, ConnectSendError> {
        let count = user_ids.len();
        if !self.chats_path_active()
            || !self.can_invite_to_bot(bot_id)
            || count == 0
            || count > crate::invite_to_bot::INVITE_LIMIT
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::Bots(BotsPurpose::InviteUsersToBot), None);
        if let Err(err) = self
            .sender
            .send_json(&invite_users_to_bot(extra, bot_id, user_ids))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.acks.expect(extra);
        Ok(extra)
    }

    /// The box closed with the invite out: its answer is no longer wanted.
    pub fn forget_bot_invite(&mut self, request: RequestId) {
        self.session.acks.forget(request);
        let _ = self.session.bots.take_invite_refusal(request);
    }
}
