//! Message menu rows that depend on where a message came from: the
//! original date of a forward, "Send Another Gift" on gift service
//! messages, and blocking the sender of a reply in the Replies chat.
//! Ports of tdesktop's `AddWhenEditedForwardedAuthorActionHelper`, the gift
//! branch of `FillContextMenu`, and `AddBlockSenderAction`
//! (`history/view/history_view_context_menu.cpp`).
#![deny(clippy::indexing_slicing)]

use crate::ids::{ChatId, MessageId};
use crate::lang::{keys, tr_with};
use crate::local_time::{CivilTime, hhmm};
use crate::state::{HistoryMessage, Session, short_date_time};
use crate::telegram::envelope::{
    GiftOrigin, MessageContent, MessageOrigin, MessageSender, ServiceAction,
};

/// The Replies chat on the production servers.
const PRODUCTION_REPLIES_CHAT_ID: i64 = 1_271_266_957;

impl Session {
    /// The chat is the Replies chat (tdesktop `PeerData::isRepliesChat`).
    /// Its id comes from the TDLib option `replies_bot_chat_id`, which
    /// differs on the test environment; until the option arrives the
    /// production id stands in, as in Telegram X.
    pub fn is_replies_chat(&self, chat_id: ChatId) -> bool {
        let id = self
            .users_state
            .replies_bot_chat_id
            .unwrap_or(PRODUCTION_REPLIES_CHAT_ID);
        id == chat_id.0
    }
}

/// "today at 12:34", "yesterday at 12:34", "Apr 15 at 00:20"
/// (`FormatReadDate`: `lng_mediaview_today`, `_yesterday`, `_date_time`).
pub fn original_date_text(date: &CivilTime, now: &CivilTime) -> String {
    let time = hhmm(date);
    match now.day_number() - date.day_number() {
        0 => tr_with(keys::MEDIAVIEW_TODAY, &[("time", &time)]),
        1 => tr_with(keys::MEDIAVIEW_YESTERDAY, &[("time", &time)]),
        _ => short_date_time(date, now),
    }
}

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
        }
        | ServiceAction::RefundedUpgradedGift {
            origin: GiftOrigin::Upgrade,
        } => !outgoing,
        ServiceAction::GiftedPremium { .. }
        | ServiceAction::GiftedStars { .. }
        | ServiceAction::GiftedGrams { .. }
        | ServiceAction::Gift { .. }
        | ServiceAction::UpgradedGift { .. }
        | ServiceAction::RefundedUpgradedGift { .. }
        | ServiceAction::GiftOffer { .. } => outgoing,
        // A code made by a channel or a giveaway is not a gift (tdesktop
        // `slug.isEmpty() || !channel`).
        ServiceAction::PremiumGiftCode {
            from_giveaway,
            creator,
            ..
        } => !from_giveaway && !matches!(creator, Some(MessageSender::Chat { .. })) && outgoing,
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
    /// A reply posted as a channel or an anonymous admin.
    Chat {
        chat_id: i64,
        signature: String,
    },
    Hidden(String),
}

/// The original sender of a message in the Replies chat, when the menu
/// offers to block them (tdesktop `item->originalSender()`).
pub fn replies_sender(message: &HistoryMessage) -> Option<RepliesSender> {
    let Some(info) = message.forward_info.as_ref() else {
        return match message.sender? {
            MessageSender::User { user_id } => Some(RepliesSender::User(user_id)),
            MessageSender::Chat { chat_id } => Some(RepliesSender::Chat {
                chat_id,
                signature: String::new(),
            }),
        };
    };
    Some(match &info.origin {
        MessageOrigin::User { user_id } => RepliesSender::User(user_id.0),
        MessageOrigin::HiddenUser { sender_name } => RepliesSender::Hidden(sender_name.clone()),
        MessageOrigin::Chat {
            chat_id,
            author_signature,
        }
        | MessageOrigin::Channel {
            chat_id,
            author_signature,
            ..
        } => RepliesSender::Chat {
            chat_id: chat_id.0,
            signature: author_signature.clone(),
        },
    })
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
        RepliesBlock, RepliesSender, original_date, original_date_text, replies_sender,
        send_another_gift_offered,
    };
    use crate::ids::{ChatId, MessageId, UserId};
    use crate::state::HistoryMessage;
    use crate::telegram::envelope::{
        GiftOrigin, MessageContent, MessageForwardInfo, MessageImportInfo, MessageOrigin,
        MessageSender, Money, ServiceAction,
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
    fn the_original_date_reads_like_the_read_time() {
        let now = crate::local_time::civil_at(1_790_631_720, 0);
        let today = crate::local_time::civil_at(1_790_631_720 - 3600, 0);
        let yesterday = crate::local_time::civil_at(1_790_631_720 - 86_400, 0);
        let older = crate::local_time::civil_at(1_790_631_720 - 10 * 86_400, 0);
        assert_eq!(original_date_text(&today, &now), "today at 20:42");
        assert_eq!(original_date_text(&yesterday, &now), "yesterday at 21:42");
        assert_eq!(original_date_text(&older, &now), "Sep 18 at 21:42");
        let far = crate::local_time::civil_at(1_790_631_720 - 400 * 86_400, 0);
        assert_eq!(original_date_text(&far, &now), "Aug 24, 2025 at 21:42");
    }

    #[test]
    fn refunded_upgrades_and_channel_codes_follow_tdesktop() {
        let refunded = |origin| action(ServiceAction::RefundedUpgradedGift { origin });
        assert!(send_another_gift_offered(
            &refunded(GiftOrigin::Upgrade),
            false
        ));
        assert!(send_another_gift_offered(
            &refunded(GiftOrigin::Transfer),
            true
        ));
        let code = |creator| {
            action(ServiceAction::PremiumGiftCode {
                creator,
                from_giveaway: false,
                unclaimed: false,
                money: money(),
            })
        };
        assert!(send_another_gift_offered(&code(None), true));
        assert!(!send_another_gift_offered(
            &code(Some(MessageSender::Chat { chat_id: -100 })),
            true
        ));
    }

    #[test]
    fn the_replies_chat_comes_from_the_option() {
        let sink: std::sync::Arc<dyn crate::diagnostics::DiagnosticSink> =
            std::sync::Arc::new(crate::diagnostics::MemorySink::new());
        let mut session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink);
        assert!(!session.is_replies_chat(ChatId(708_513)));
        assert!(session.is_replies_chat(ChatId(1_271_266_957)));
        session.users_state.apply_option(
            "replies_bot_chat_id",
            &crate::telegram::envelope::OptionValue::Integer(708_513),
        );
        assert!(session.is_replies_chat(ChatId(708_513)));
        assert!(!session.is_replies_chat(ChatId(1_271_266_957)));
    }

    #[test]
    fn a_channel_reply_names_the_channel() {
        let mut message = base();
        message.forward_info = Some(MessageForwardInfo {
            origin: MessageOrigin::Channel {
                chat_id: ChatId(-100),
                message_id: MessageId(1),
                author_signature: "sig".into(),
            },
            date: 1,
            source: None,
            psa_type: String::new(),
        });
        assert_eq!(
            replies_sender(&message),
            Some(RepliesSender::Chat {
                chat_id: -100,
                signature: "sig".into()
            })
        );
    }

    #[test]
    fn the_block_box_starts_with_everything_checked() {
        let block = RepliesBlock::new(MessageId(3), "Ada".into(), "Ada L".into(), true);
        assert!(block.report_spam && block.delete_message && block.delete_all);
    }
}
