//! Message menu rows that depend on where a message came from: the
//! original date of a forward, "Send Another Gift" on gift service
//! messages, and blocking the sender of a reply in the Replies chat.
//! Ports of tdesktop's `AddWhenEditedForwardedAuthorActionHelper`, the gift
//! branch of `FillContextMenu`, and `AddBlockSenderAction`
//! (`history/view/history_view_context_menu.cpp`).
#![deny(clippy::indexing_slicing)]

use crate::ids::MessageId;
use crate::state::HistoryMessage;
use crate::telegram::envelope::{GiftOrigin, MessageContent, MessageOrigin, ServiceAction};

/// The user id of the Replies chat (tdesktop `PeerData::isRepliesChat`).
pub const REPLIES_USER_ID: i64 = 1_271_266_957;

/// When the message was first sent, for a forward or an imported message.
/// tdesktop skips announcements (`psaType`) and forwarded stories; TDLib
/// delivers stories as their own content, so only the PSA check remains.
pub fn original_date(message: &HistoryMessage) -> Option<i32> {
    let date = match (&message.forward_info, &message.extras.import_info) {
        (Some(info), _) if !info.psa_type.is_empty() => return None,
        (Some(info), _) => info.date,
        (None, Some(import)) => import.date,
        (None, None) => return None,
    };
    (date > 0).then_some(date)
}

/// Whether the gift service message offers "Send Another Gift" (the
/// content part of tdesktop's check; the recipient check is the caller's).
/// A gift shows it on the sender's side. An upgrade of a gift shows it for
/// the other side, because the upgrade message is "outgoing" for whoever
/// paid for it. Giveaway prizes are not gifts.
pub fn send_another_gift_offered(content: &MessageContent, outgoing: bool) -> bool {
    let MessageContent::Action(action) = content else {
        return false;
    };
    let action = match action.as_ref() {
        ServiceAction::WithCard { action, .. } => action.as_ref(),
        other => other,
    };
    match action {
        ServiceAction::UpgradedGift {
            origin: GiftOrigin::Upgrade,
            ..
        } => !outgoing,
        ServiceAction::GiftedPremium { .. }
        | ServiceAction::GiftedStars { .. }
        | ServiceAction::GiftedGrams { .. }
        | ServiceAction::Gift { .. }
        | ServiceAction::UpgradedGift { .. }
        | ServiceAction::GiftOffer { .. } => outgoing,
        ServiceAction::PremiumGiftCode { from_giveaway, .. } => !from_giveaway && outgoing,
        ServiceAction::Theme {
            gift_name: Some(_), ..
        } => outgoing,
        _ => false,
    }
}

/// Who a reply in the Replies chat came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepliesSender {
    User(i64),
    Hidden(String),
}

/// The original sender of a message in the Replies chat, when the menu
/// offers to block them (tdesktop `item->originalSender()`).
pub fn replies_sender(message: &HistoryMessage) -> Option<RepliesSender> {
    match &message.forward_info.as_ref()?.origin {
        MessageOrigin::User { user_id } => Some(RepliesSender::User(user_id.0)),
        MessageOrigin::HiddenUser { sender_name } => {
            Some(RepliesSender::Hidden(sender_name.clone()))
        }
        _ => None,
    }
}

/// The open "Block {name}" box for a reply: the three checkboxes
/// (`lng_report_spam`, `lng_blocked_list_confirm_reply`,
/// `lng_blocked_list_confirm_reply_all`), all checked at first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepliesBlock {
    pub message_id: MessageId,
    /// The short name in the title and the toast.
    pub name: String,
    /// The full name in "Delete all replies from {user}".
    pub full_name: String,
    /// A hidden sender is not a user: no confirmation line (tdesktop only
    /// adds it for a `UserData`).
    pub is_user: bool,
    pub report_spam: bool,
    pub delete_message: bool,
    pub delete_all: bool,
}

impl RepliesBlock {
    pub fn new(message_id: MessageId, name: String, full_name: String, is_user: bool) -> Self {
        Self {
            message_id,
            name,
            full_name,
            is_user,
            report_spam: true,
            delete_message: true,
            delete_all: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        REPLIES_USER_ID, RepliesBlock, RepliesSender, original_date, replies_sender,
        send_another_gift_offered,
    };
    use crate::ids::{ChatId, MessageId, UserId};
    use crate::state::HistoryMessage;
    use crate::telegram::envelope::{
        GiftOrigin, MessageContent, MessageForwardInfo, MessageImportInfo, MessageOrigin, Money,
        ServiceAction,
    };

    fn base() -> HistoryMessage {
        HistoryMessage {
            sender: None,
            id: MessageId(5),
            chat_id: ChatId(1),
            is_outgoing: false,
            date: 100,
            content: MessageContent::Unsupported {
                type_name: String::new(),
            },
            pending: false,
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            author_signature: None,
            failed: false,
            can_retry: false,
            ephemeral: None,
        }
    }

    fn fwd(psa: &str, date: i32) -> Option<MessageForwardInfo> {
        Some(MessageForwardInfo {
            origin: MessageOrigin::User { user_id: UserId(9) },
            date,
            source: None,
            psa_type: psa.into(),
        })
    }

    #[test]
    fn original_date_needs_a_plain_forward_or_an_import() {
        let mut message = base();
        assert_eq!(original_date(&message), None);
        message.forward_info = fwd("", 50);
        assert_eq!(original_date(&message), Some(50));
        message.forward_info = fwd("covid", 50);
        assert_eq!(original_date(&message), None);
        message.forward_info = None;
        message.extras.import_info = Some(MessageImportInfo {
            sender_name: "A".into(),
            date: 7,
        });
        assert_eq!(original_date(&message), Some(7));
    }

    fn money() -> Money {
        Money {
            currency: "XTR".into(),
            amount: 1,
        }
    }

    fn action(a: ServiceAction) -> MessageContent {
        MessageContent::Action(Box::new(a))
    }

    #[test]
    fn another_gift_follows_the_senders_side() {
        let gift = action(ServiceAction::GiftedStars {
            gifter: 1,
            receiver: 2,
            money: money(),
        });
        assert!(send_another_gift_offered(&gift, true));
        assert!(!send_another_gift_offered(&gift, false));
        let upgrade = action(ServiceAction::UpgradedGift {
            name: "x".into(),
            sender: None,
            receiver: None,
            origin: GiftOrigin::Upgrade,
            price: None,
        });
        assert!(send_another_gift_offered(&upgrade, false));
        assert!(!send_another_gift_offered(&upgrade, true));
        let giveaway = action(ServiceAction::PremiumGiftCode {
            creator: None,
            from_giveaway: true,
            unclaimed: false,
            money: money(),
        });
        assert!(!send_another_gift_offered(&giveaway, true));
        assert!(!send_another_gift_offered(&base().content, true));
    }

    #[test]
    fn replies_sender_is_the_forward_origin_user() {
        let mut message = base();
        assert_eq!(replies_sender(&message), None);
        message.forward_info = fwd("", 1);
        assert_eq!(replies_sender(&message), Some(RepliesSender::User(9)));
        message.forward_info = Some(MessageForwardInfo {
            origin: MessageOrigin::HiddenUser {
                sender_name: "Ada".into(),
            },
            date: 1,
            source: None,
            psa_type: String::new(),
        });
        assert_eq!(
            replies_sender(&message),
            Some(RepliesSender::Hidden("Ada".into()))
        );
    }

    #[test]
    fn the_block_box_starts_with_everything_checked() {
        let block = RepliesBlock::new(MessageId(3), "Ada".into(), "Ada L".into(), true);
        assert!(block.report_spam && block.delete_message && block.delete_all);
        assert_eq!(REPLIES_USER_ID, 1_271_266_957);
    }
}
