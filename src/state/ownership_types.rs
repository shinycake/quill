//! Chat ownership state: the `canTransferOwnership` gate, the
//! `transferChatOwnership` round trip and the "who inherits when the owner
//! leaves" lookup (`getChatOwnerAfterLeaving`). tdesktop:
//! `boxes/peers/channel_ownership_transfer.cpp`,
//! `boxes/select_future_owner_box.cpp`.
use super::*;

/// Answer of `getChatOwnerAfterLeaving` for one chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerLookup {
    Loading,
    /// The user who becomes owner (or the next admin) after the viewer
    /// leaves.
    Loaded(i64),
    Failed(String),
}

/// One transfer the viewer started and has not seen the end of yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnershipTransfer {
    pub chat_id: i64,
    pub user_id: i64,
}

/// The viewer's own standing in a basic group, from `updateBasicGroup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasicGroupOwn {
    pub status: ChannelMemberStatus,
    pub can_restrict_members: bool,
    pub can_promote_members: bool,
    pub can_manage_tags: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnershipState {
    /// Last `canTransferOwnership` answer; `None` until asked.
    pub can_transfer: Option<CanTransferOwnershipResult>,
    pub check_in_flight: bool,
    pub check_error: Option<String>,
    /// A `transferChatOwnership` is waiting for TDLib.
    pub transfer_in_flight: Option<OwnershipTransfer>,
    /// Why the last transfer failed (wrong password, flood wait). Never
    /// holds the password.
    pub transfer_error: Option<String>,
    /// One-shot: the transfer that just succeeded; the UI drains it.
    pub transferred: Option<OwnershipTransfer>,
    pub owner_after_leaving: HashMap<i64, OwnerLookup>,
}

/// Why `canTransferOwnership` blocks a transfer, in tdesktop's words
/// (`lng_rights_transfer_check_*`), or `None` when it may go ahead.
pub fn transfer_block_reason(result: CanTransferOwnershipResult) -> Option<String> {
    match result {
        CanTransferOwnershipResult::Ok => None,
        CanTransferOwnershipResult::PasswordNeeded => Some(
            "Enable Two-Step Verification in Settings and wait 7 days before you transfer ownership."
                .to_string(),
        ),
        CanTransferOwnershipResult::PasswordTooFresh { retry_after } => Some(format!(
            "Two-Step Verification was enabled too recently. Please come back in {}.",
            wait_text(retry_after)
        )),
        CanTransferOwnershipResult::SessionTooFresh { retry_after } => Some(format!(
            "You logged in on this device too recently. Please come back in {}.",
            wait_text(retry_after)
        )),
    }
}

fn wait_text(secs: i32) -> String {
    let secs = i64::from(secs.max(0));
    if secs >= 2 * 86_400 {
        format!("{} days", (secs + 86_399) / 86_400)
    } else if secs >= 3_600 {
        let hours = (secs + 3_599) / 3_600;
        if hours == 1 {
            "1 hour".to_string()
        } else {
            format!("{hours} hours")
        }
    } else {
        "a little while".to_string()
    }
}

impl Session {
    /// A `canTransferOwnership` request went out.
    pub fn begin_ownership_check(&mut self) {
        self.ownership.can_transfer = None;
        self.ownership.check_in_flight = true;
        self.ownership.check_error = None;
    }

    pub(crate) fn accept_can_transfer_ownership(&mut self, result: CanTransferOwnershipResult) {
        self.ownership.can_transfer = Some(result);
        self.ownership.check_in_flight = false;
        self.ownership.check_error = None;
    }

    /// The transfer left for TDLib; clears any earlier failure.
    pub fn begin_ownership_transfer(&mut self, chat_id: i64, user_id: i64) {
        self.ownership.transfer_in_flight = Some(OwnershipTransfer { chat_id, user_id });
        self.ownership.transfer_error = None;
    }

    pub(crate) fn finish_ownership_transfer(&mut self, user_id: i64, chat_id: Option<i64>) {
        let done = self
            .ownership
            .transfer_in_flight
            .take()
            .or_else(|| chat_id.map(|chat_id| OwnershipTransfer { chat_id, user_id }));
        self.ownership.transferred = done;
        // The viewer is now a plain admin; TDLib confirms with
        // `updateChatMember`, but the cached admin list is stale already.
        if let Some(done) = done {
            self.admin_lists.remove(&done.chat_id);
            self.supergroup_members
                .retain(|(id, _), _| *id != done.chat_id);
            self.basic_group_members.remove(&done.chat_id);
        }
    }

    pub(crate) fn fail_ownership_transfer(&mut self, err: &TdError) {
        self.ownership.transfer_in_flight = None;
        self.ownership.transfer_error = Some(ownership_error_line(err));
    }

    /// The admin's `deleteMessageReactionsFromSender` was confirmed: the
    /// member leaves the who-reacted list of that message.
    pub(crate) fn drop_reactor_from_audience(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        user_id: i64,
    ) {
        let Some(audience) = self
            .message_audience
            .as_mut()
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id)
        else {
            return;
        };
        if let Audience::Ready(page) = &mut audience.reactions {
            let before = page.reactions.len();
            page.reactions
                .retain(|r| r.sender != MessageSender::User { user_id });
            let removed = (before - page.reactions.len()) as i32;
            page.total_count = (page.total_count - removed).max(0);
        }
    }

    /// Mark the "who inherits" lookup of `chat_id` as running.
    pub fn begin_owner_lookup(&mut self, chat_id: i64) {
        self.ownership
            .owner_after_leaving
            .insert(chat_id, OwnerLookup::Loading);
    }

    pub(crate) fn accept_owner_after_leaving(&mut self, chat_id: i64, user_id: i64) {
        self.ownership
            .owner_after_leaving
            .insert(chat_id, OwnerLookup::Loaded(user_id));
    }

    pub(crate) fn fail_owner_lookup(&mut self, chat_id: i64, err: &TdError) {
        self.ownership.owner_after_leaving.insert(
            chat_id,
            OwnerLookup::Failed(format!(
                "Could not find the next owner: {}",
                error_reason(err)
            )),
        );
    }
}

/// One honest line for a failed `transferChatOwnership`. TDLib's message
/// is never stored, so a 400 covers "wrong password" and "cannot be
/// transferred" alike.
pub(crate) fn ownership_error_line(err: &TdError) -> String {
    match err.class {
        ErrorClass::Flood => err.flood_line("Too many attempts. Wait and try again."),
        ErrorClass::Unauthorized => "This session is no longer authorized.".to_string(),
        ErrorClass::Invalid => {
            "Wrong password, or the ownership can't be transferred to this person.".to_string()
        }
        _ => format!("Could not transfer ownership (error {}).", err.code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deleted_reaction_leaves_the_who_reacted_list() {
        use crate::telegram::envelope::{AddedReaction, AddedReactionsPage, ReactionType};
        let sink = std::sync::Arc::new(crate::diagnostics::MemorySink::new());
        let mut session = Session::new(crate::ids::AccountKey::primary(), sink);
        let (chat, msg) = (ChatId(-100), MessageId(50));
        let reaction = |user_id| AddedReaction {
            reaction_type: ReactionType::emoji("👍"),
            sender: MessageSender::User { user_id },
            is_outgoing: false,
            date: 0,
        };
        let mut audience = MessageAudience::new(chat, msg);
        audience.reactions = Audience::Ready(AddedReactionsPage {
            total_count: 2,
            reactions: vec![reaction(8), reaction(9)],
            next_offset: String::new(),
        });
        session.message_audience = Some(audience);
        session.drop_reactor_from_audience(chat, msg, 8);
        let page = session
            .message_audience
            .as_ref()
            .unwrap()
            .reactions
            .ready()
            .unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.reactions.len(), 1);
        // Another message's list is left alone.
        session.drop_reactor_from_audience(chat, MessageId(51), 9);
        assert_eq!(
            session
                .message_audience
                .unwrap()
                .reactions
                .ready()
                .unwrap()
                .reactions
                .len(),
            1
        );
    }

    #[test]
    fn fresh_password_and_session_explain_the_wait() {
        assert!(transfer_block_reason(CanTransferOwnershipResult::Ok).is_none());
        let line = transfer_block_reason(CanTransferOwnershipResult::PasswordTooFresh {
            retry_after: 3 * 86_400,
        })
        .unwrap();
        assert!(line.contains("3 days"), "{line}");
        let line = transfer_block_reason(CanTransferOwnershipResult::SessionTooFresh {
            retry_after: 5_400,
        })
        .unwrap();
        assert!(line.contains("2 hours"), "{line}");
        assert!(
            transfer_block_reason(CanTransferOwnershipResult::PasswordNeeded)
                .unwrap()
                .contains("Two-Step")
        );
    }
}
