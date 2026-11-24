//! Inviting people to a bot the user owns (TDLib 1.8.71 `inviteUsersToBot`,
//! tdesktop `boxes/peers/invite_to_bot_box.cpp`): a list of the people the
//! user has chats with, up to [`INVITE_LIMIT`] ticked, then Invite. Each
//! invited person gets a `messageBotInvite` from the bot.
//!
//! The box is reached from the bot's profile ("Invite to Bot") and from a
//! `t.me/botinvite/<bot>` link (`internalLinkTypeBotInvite`), both only for
//! a bot whose `userTypeBot.can_be_edited` is true.
#![deny(clippy::indexing_slicing)]

use crate::ids::RequestId;
use crate::telegram::envelope::TdError;

/// tdesktop `kInviteToBotLimit`; TDLib takes 1-10 users.
pub const INVITE_LIMIT: usize = 10;

/// The open box: the bot, the people ticked in the order they were ticked,
/// and whether the invite is out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteToBot {
    pub bot_id: i64,
    picked: Vec<i64>,
    pub stage: InviteStage,
}

/// Where the box is. Leaving `Sending` takes the answer to `request`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteStage {
    Picking,
    Sending { request: RequestId },
}

/// What a tick did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggled {
    Changed,
    /// The 11th person: nothing changed, tdesktop shows
    /// `lng_bot_invite_limit`.
    AtLimit,
    /// The invite is out: ticks wait for its answer.
    Unchanged,
}

/// Why TDLib refused the invite (tdesktop `ShowInviteError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InviteRefusal {
    /// `BOT_INVITE_ALREADY_STARTED`: the people already have a chat with
    /// the bot.
    AlreadyStarted,
    /// `BOT_INVITE_FAILED`: none of them could be invited.
    Failed,
    /// Anything else, as the error line reads it.
    Other(String),
}

impl InviteRefusal {
    /// `reason` is the error line for any other refusal.
    pub fn of(err: &TdError, reason: impl FnOnce(&TdError) -> String) -> Self {
        match err.server_type.as_deref() {
            Some("BOT_INVITE_ALREADY_STARTED") => Self::AlreadyStarted,
            Some("BOT_INVITE_FAILED") => Self::Failed,
            _ => Self::Other(reason(err)),
        }
    }
}

impl InviteToBot {
    pub fn new(bot_id: i64) -> Self {
        Self {
            bot_id,
            picked: Vec::new(),
            stage: InviteStage::Picking,
        }
    }

    pub fn picked(&self) -> &[i64] {
        &self.picked
    }

    pub fn is_picked(&self, user_id: i64) -> bool {
        self.picked.contains(&user_id)
    }

    pub fn toggle(&mut self, user_id: i64) -> Toggled {
        if self.stage != InviteStage::Picking {
            return Toggled::Unchanged;
        }
        if let Some(at) = self.picked.iter().position(|id| *id == user_id) {
            self.picked.remove(at);
            return Toggled::Changed;
        }
        if self.picked.len() >= INVITE_LIMIT {
            return Toggled::AtLimit;
        }
        self.picked.push(user_id);
        Toggled::Changed
    }

    /// "Invite" is offered: someone is ticked and nothing is out.
    pub fn can_send(&self) -> bool {
        !self.picked.is_empty() && self.stage == InviteStage::Picking
    }

    /// The request that is out, if any.
    pub fn waits_for(&self) -> Option<RequestId> {
        match self.stage {
            InviteStage::Sending { request } => Some(request),
            InviteStage::Picking => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_stop_at_ten_and_untick_frees_a_place() {
        let mut invite = InviteToBot::new(7);
        for id in 1..=10 {
            assert_eq!(invite.toggle(id), Toggled::Changed);
        }
        assert_eq!(invite.toggle(11), Toggled::AtLimit);
        assert!(!invite.is_picked(11));
        assert_eq!(invite.toggle(3), Toggled::Changed);
        assert_eq!(invite.toggle(11), Toggled::Changed);
        assert_eq!(invite.picked().len(), INVITE_LIMIT);
    }

    #[test]
    fn nothing_changes_while_the_invite_is_out() {
        let mut invite = InviteToBot::new(7);
        assert!(!invite.can_send());
        invite.toggle(1);
        assert!(invite.can_send());
        invite.stage = InviteStage::Sending {
            request: RequestId(5),
        };
        assert_eq!(invite.toggle(2), Toggled::Unchanged);
        assert!(!invite.can_send());
        assert_eq!(invite.waits_for(), Some(RequestId(5)));
    }
}
