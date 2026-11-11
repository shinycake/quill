//! Chat ownership state: the `canTransferOwnership` gate, the
//! `transferChatOwnership` round trip and the "who inherits when the owner
//! leaves" lookup (`getChatOwnerAfterLeaving`). tdesktop:
//! `boxes/peers/channel_ownership_transfer.cpp`,
//! `boxes/select_future_owner_box.cpp`.
use super::*;
use crate::lang::{keys, tr, tr_with, trn};
use crate::state::fetch::Fetch;

/// Answer of `getChatOwnerAfterLeaving` for one chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerLookup {
    /// `getChatOwnerAfterLeaving` in flight as this request: only its
    /// answer or error counts.
    Loading(RequestId),
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
    /// The viewer asked to leave once the new owner is appointed. It rides
    /// with the transfer it was asked for, so the answer never reads
    /// whatever box is open by then.
    pub leave_after: bool,
}

/// The viewer's own standing in a basic group, from `updateBasicGroup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasicGroupOwn {
    pub status: ChannelMemberStatus,
    pub can_restrict_members: bool,
    pub can_promote_members: bool,
    pub can_manage_tags: bool,
}

/// Where the viewer's `transferChatOwnership` stands. Each stage leaves
/// by the answer, the error, or the dialog opening again.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TransferStage {
    #[default]
    Idle,
    /// Waiting for TDLib: only the answer or error of `request` counts.
    Sending {
        request: RequestId,
        transfer: OwnershipTransfer,
    },
    /// Why the transfer failed (wrong password, flood wait). Never holds
    /// the password.
    Failed(String),
    /// The transfer succeeded; the UI drains it once.
    Done(OwnershipTransfer),
}

impl TransferStage {
    /// The transfer waiting for TDLib.
    pub fn sending(&self) -> Option<&OwnershipTransfer> {
        match self {
            Self::Sending { transfer, .. } => Some(transfer),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// Whether the transfer in flight is the one `request` asked.
    pub fn waits_for(&self, request: Option<RequestId>) -> bool {
        matches!(self, Self::Sending { request: asked, .. } if Some(*asked) == request)
    }

    /// A finished transfer or its failure is forgotten; one in flight stays.
    pub fn forget_settled(&mut self) {
        if !matches!(self, Self::Sending { .. }) {
            *self = Self::Idle;
        }
    }

    /// The succeeded transfer, once; any other stage stays as it is.
    pub fn take_done(&mut self) -> Option<OwnershipTransfer> {
        match std::mem::take(self) {
            Self::Done(transfer) => Some(transfer),
            other => {
                *self = other;
                None
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnershipState {
    /// The `canTransferOwnership` gate: asked, answered, or failed.
    pub check: Fetch<CanTransferOwnershipResult>,
    pub transfer: TransferStage,
    pub owner_after_leaving: HashMap<i64, OwnerLookup>,
}

/// Why `canTransferOwnership` blocks a transfer, in tdesktop's words
/// (`lng_rights_transfer_check_*`), or `None` when it may go ahead.
pub fn transfer_block_reason(result: CanTransferOwnershipResult) -> Option<String> {
    match result {
        CanTransferOwnershipResult::Ok => None,
        CanTransferOwnershipResult::PasswordNeeded => {
            Some(tr(keys::Q_OPS_ERR_TRANSFER_NEED_2FA).to_string())
        }
        CanTransferOwnershipResult::PasswordTooFresh { retry_after } => Some(tr_with(
            keys::Q_OPS_ERR_TRANSFER_2FA_FRESH,
            &[("wait", &wait_text(retry_after))],
        )),
        CanTransferOwnershipResult::SessionTooFresh { retry_after } => Some(tr_with(
            keys::Q_OPS_ERR_TRANSFER_SESSION_FRESH,
            &[("wait", &wait_text(retry_after))],
        )),
    }
}

fn wait_text(secs: i32) -> String {
    let secs = i64::from(secs.max(0));
    if secs >= 2 * 86_400 {
        trn(keys::DAYS, (secs + 86_399) / 86_400)
    } else if secs >= 3_600 {
        let hours = (secs + 3_599) / 3_600;
        trn(keys::HOURS, hours)
    } else {
        tr(keys::Q_OPS_ERR_WAIT_LITTLE).to_string()
    }
}

impl Session {
    /// A `canTransferOwnership` request went out as `request`.
    pub fn begin_ownership_check(&mut self, request: RequestId) {
        self.groups.ownership.check = Fetch::Loading {
            stale: None,
            request: Some(request),
        };
    }

    /// The answer of `request`; an older request's late answer changes
    /// nothing.
    pub(crate) fn accept_can_transfer_ownership(
        &mut self,
        request: Option<RequestId>,
        result: CanTransferOwnershipResult,
    ) {
        self.groups.ownership.check.answered(request, result);
    }

    /// The transfer left for TDLib as `request`; clears any earlier failure.
    pub fn begin_ownership_transfer(
        &mut self,
        request: RequestId,
        chat_id: i64,
        user_id: i64,
        leave_after: bool,
    ) {
        self.groups.ownership.transfer = TransferStage::Sending {
            request,
            transfer: OwnershipTransfer {
                chat_id,
                user_id,
                leave_after,
            },
        };
    }

    /// `request` came back `ok`: the transfer happened. The box waiting for
    /// it is done; with none waiting for this request (the dialog opened
    /// again meanwhile) another transfer in flight stays, and a finished
    /// one is recorded from the request's own chat.
    pub(crate) fn finish_ownership_transfer(
        &mut self,
        request: Option<RequestId>,
        user_id: i64,
        chat_id: Option<i64>,
    ) {
        let waiting = match &self.groups.ownership.transfer {
            TransferStage::Sending {
                request: asked,
                transfer,
            } if Some(*asked) == request => Some(*transfer),
            _ => None,
        };
        let other_in_flight =
            self.groups.ownership.transfer.sending().is_some() && waiting.is_none();
        let done = waiting.or_else(|| {
            chat_id.map(|chat_id| OwnershipTransfer {
                chat_id,
                user_id,
                leave_after: false,
            })
        });
        if let Some(done) = done
            && !other_in_flight
        {
            self.groups.ownership.transfer = TransferStage::Done(done);
        }
        // The viewer is now a plain admin; TDLib confirms with
        // `updateChatMember`, but the cached admin list is stale already.
        if let Some(done) = done {
            self.groups.admin_lists.remove(&done.chat_id);
            self.groups
                .supergroup_members
                .retain(|(id, _), _| *id != done.chat_id);
            self.groups.basic_group_members.remove(&done.chat_id);
            // The lists on screen ask again (the poll's refetch), instead
            // of drawing the old badges under "Loading..." for good.
            self.groups.mark_member_list_stale(done.chat_id);
        }
    }

    /// `request` failed: only the transfer waiting for it fails.
    pub(crate) fn fail_ownership_transfer(&mut self, request: Option<RequestId>, err: &TdError) {
        if self.groups.ownership.transfer.waits_for(request) {
            self.groups.ownership.transfer = TransferStage::Failed(ownership_error_line(err));
        }
    }

    /// The admin's `deleteMessageReactionsFromSender` was confirmed: the
    /// member leaves the who-reacted list of that message.
    pub(crate) fn drop_reactor_from_audience(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        sender: MessageSender,
    ) {
        let Some(audience) = self
            .messages
            .message_audience
            .as_mut()
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id)
        else {
            return;
        };
        if let Audience::Ready(page) = &mut audience.reactions {
            let before = page.reactions.len();
            page.reactions.retain(|r| r.sender != sender);
            let removed = (before - page.reactions.len()) as i32;
            page.total_count = (page.total_count - removed).max(0);
        }
    }

    /// Mark the "who inherits" lookup of `chat_id` as running as `request`.
    pub fn begin_owner_lookup(&mut self, chat_id: i64, request: RequestId) {
        self.groups
            .ownership
            .owner_after_leaving
            .insert(chat_id, OwnerLookup::Loading(request));
    }

    /// The lookup of `chat_id` waiting for `request`, if it still is.
    fn owner_lookup_waits(&self, chat_id: i64, request: Option<RequestId>) -> bool {
        matches!(
            self.groups.ownership.owner_after_leaving.get(&chat_id),
            Some(OwnerLookup::Loading(asked)) if Some(*asked) == request
        )
    }

    pub(crate) fn accept_owner_after_leaving(
        &mut self,
        chat_id: i64,
        request: Option<RequestId>,
        user_id: i64,
    ) {
        if self.owner_lookup_waits(chat_id, request) {
            self.groups
                .ownership
                .owner_after_leaving
                .insert(chat_id, OwnerLookup::Loaded(user_id));
        }
    }

    pub(crate) fn fail_owner_lookup(
        &mut self,
        chat_id: i64,
        request: Option<RequestId>,
        err: &TdError,
    ) {
        if self.owner_lookup_waits(chat_id, request) {
            self.groups.ownership.owner_after_leaving.insert(
                chat_id,
                OwnerLookup::Failed(tr_with(
                    keys::Q_OPS_ERR_OWNER_LOOKUP_FAILED,
                    &[("reason", &error_reason(err))],
                )),
            );
        }
    }
}

/// One honest line for a failed `transferChatOwnership`. TDLib's message
/// is never stored, so a 400 covers "wrong password" and "cannot be
/// transferred" alike.
pub(crate) fn ownership_error_line(err: &TdError) -> String {
    match err.class {
        ErrorClass::Flood => err.flood_line(tr(keys::Q_OPS_ERR_TRANSFER_FLOOD)),
        ErrorClass::Unauthorized => tr(keys::Q_OPS_ERR_TRANSFER_UNAUTHORIZED).to_string(),
        ErrorClass::Invalid => tr(keys::Q_OPS_ERR_TRANSFER_INVALID).to_string(),
        _ => tr_with(
            keys::Q_OPS_ERR_TRANSFER_CODE,
            &[("code", &err.code.to_string())],
        ),
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
        session.messages.message_audience = Some(audience);
        session.drop_reactor_from_audience(chat, msg, MessageSender::User { user_id: 8 });
        let page = session
            .messages
            .message_audience
            .as_ref()
            .unwrap()
            .reactions
            .ready()
            .unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.reactions.len(), 1);
        // Another message's list is left alone.
        session.drop_reactor_from_audience(chat, MessageId(51), MessageSender::User { user_id: 9 });
        assert_eq!(
            session
                .messages
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

    /// Round 17 (area B, finding 9): a channel's or an anonymous admin's
    /// row goes too; it was matched as user 0 before.
    #[test]
    fn a_deleted_channel_reaction_leaves_the_who_reacted_list() {
        use crate::telegram::envelope::{AddedReaction, AddedReactionsPage, ReactionType};
        let sink = std::sync::Arc::new(crate::diagnostics::MemorySink::new());
        let mut session = Session::new(crate::ids::AccountKey::primary(), sink);
        let (chat, msg) = (ChatId(-100), MessageId(50));
        let reaction = |sender| AddedReaction {
            reaction_type: ReactionType::emoji("👍"),
            sender,
            is_outgoing: false,
            date: 0,
        };
        let channel = MessageSender::Chat { chat_id: -200 };
        let mut audience = MessageAudience::new(chat, msg);
        audience.reactions = Audience::Ready(AddedReactionsPage {
            total_count: 2,
            reactions: vec![
                reaction(channel),
                reaction(MessageSender::User { user_id: 8 }),
            ],
            next_offset: String::new(),
        });
        session.messages.message_audience = Some(audience);
        session.drop_reactor_from_audience(chat, msg, channel);
        let page = session
            .messages
            .message_audience
            .as_ref()
            .and_then(|audience| audience.reactions.ready());
        assert_eq!(page.map(|page| page.total_count), Some(1));
        assert_eq!(
            page.map(|page| page.reactions.iter().map(|r| r.sender).collect::<Vec<_>>()),
            Some(vec![MessageSender::User { user_id: 8 }])
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
