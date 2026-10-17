//! The reply and reaction buttons beside a hovered bubble (tdesktop
//! `ListWidget::replyButtonParameters` / `reactionButtonParameters`, and
//! the `cornerReply` / `cornerReaction` settings). Kept pure so it is unit
//! tested.
#![deny(clippy::indexing_slicing)]

use crate::ids::{ChatId, MessageId};

/// One button's width plus the gap after it.
const SLOT: f32 = 28.;
/// Margin between the bubble and the first button.
const MARGIN: f32 = 6.;

/// Which of the (reply, react) buttons show. `prefs` is the two settings
/// (both on by default, as in Telegram Desktop; unknown means defaults).
/// Reply needs a sent message, react one that can take reactions.
pub fn visible(prefs: Option<(bool, bool)>, replyable: bool, reactable: bool) -> (bool, bool) {
    let (reply, react) = prefs.unwrap_or((true, true));
    (reply && replyable, react && reactable)
}

/// Room the buttons take beside the bubble: the "..." button always, plus
/// one slot for each extra.
pub fn span(fast_share: bool, reply: bool, react: bool) -> f32 {
    let buttons = 1 + usize::from(fast_share) + usize::from(reply) + usize::from(react);
    MARGIN + SLOT * buttons as f32
}

/// The kind of chat a message sits in, as far as the fast share button
/// cares (tdesktop `Message::displayFastShare`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastShareChat {
    /// A broadcast channel (a supergroup that is not a megagroup).
    Channel,
    /// A one-to-one chat; `bot` when the other side is a bot.
    Private { bot: bool },
    /// Groups, megagroups, secret chats and anything unknown.
    Other,
}

/// What the fast share decision reads from a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastShareFacts {
    pub chat: FastShareChat,
    /// The server has the message (not still sending).
    pub sent: bool,
    pub outgoing: bool,
    /// Forwarding is allowed: not protected content, not a secret chat.
    pub forwardable: bool,
    /// The message is a forward whose original sender is a broadcast channel.
    pub forwarded_from_channel: bool,
    /// Where a forward was made from (tdesktop `savedFromPeer` and
    /// `savedFromMsgId`), set for forwards in Saved Messages.
    pub forward_source: Option<(ChatId, MessageId)>,
    /// The content is media that a bot sent (tdesktop `allowsFastShare`).
    pub media: bool,
}

/// The round button beside the bubble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastShare {
    /// Open the forward picker for the message.
    Share,
    /// Jump to the message this one was forwarded from.
    GoToOriginal(ChatId, MessageId),
}

/// Whether the message gets the fast share button, and which one. Mirrors
/// `displayGoToOriginal` (a forward that remembers its source, wins on
/// click) and `displayFastShare` (channel posts; forwards from channels and
/// bot media in private chats), the latter only when forwarding is allowed.
pub fn fast_share(facts: FastShareFacts) -> Option<FastShare> {
    if !facts.sent {
        return None;
    }
    if let Some((chat_id, message_id)) = facts.forward_source {
        return Some(FastShare::GoToOriginal(chat_id, message_id));
    }
    if !facts.forwardable {
        return None;
    }
    let shown = match facts.chat {
        FastShareChat::Channel => true,
        FastShareChat::Private { bot } => {
            !facts.outgoing && (facts.forwarded_from_channel || (bot && facts.media))
        }
        FastShareChat::Other => false,
    };
    shown.then_some(FastShare::Share)
}

#[cfg(test)]
mod tests {
    use super::{FastShare, FastShareChat, FastShareFacts, fast_share, span, visible};
    use crate::ids::{ChatId, MessageId};

    fn facts(chat: FastShareChat) -> FastShareFacts {
        FastShareFacts {
            chat,
            sent: true,
            outgoing: false,
            forwardable: true,
            forwarded_from_channel: false,
            forward_source: None,
            media: false,
        }
    }

    #[test]
    fn channel_posts_get_the_share_button() {
        assert_eq!(
            fast_share(facts(FastShareChat::Channel)),
            Some(FastShare::Share)
        );
    }

    #[test]
    fn protected_or_unsent_channel_posts_do_not() {
        let mut f = facts(FastShareChat::Channel);
        f.forwardable = false;
        assert_eq!(fast_share(f), None);
        f.forwardable = true;
        f.sent = false;
        assert_eq!(fast_share(f), None);
    }

    #[test]
    fn groups_never_get_it() {
        assert_eq!(fast_share(facts(FastShareChat::Other)), None);
    }

    #[test]
    fn private_chats_share_channel_forwards_and_bot_media() {
        let private = |bot| facts(FastShareChat::Private { bot });
        assert_eq!(fast_share(private(false)), None);
        let mut forwarded = private(false);
        forwarded.forwarded_from_channel = true;
        assert_eq!(fast_share(forwarded), Some(FastShare::Share));
        forwarded.outgoing = true;
        assert_eq!(fast_share(forwarded), None);
        let mut bot = private(true);
        assert_eq!(fast_share(bot), None);
        bot.media = true;
        assert_eq!(fast_share(bot), Some(FastShare::Share));
        bot.forwardable = false;
        assert_eq!(fast_share(bot), None);
    }

    #[test]
    fn forwards_with_a_source_go_to_the_original_even_when_protected() {
        let mut f = facts(FastShareChat::Private { bot: false });
        f.forward_source = Some((ChatId(7), MessageId(9)));
        f.forwardable = false;
        assert_eq!(
            fast_share(f),
            Some(FastShare::GoToOriginal(ChatId(7), MessageId(9)))
        );
        f.sent = false;
        assert_eq!(fast_share(f), None);
    }

    #[test]
    fn both_buttons_show_by_default() {
        assert_eq!(visible(None, true, true), (true, true));
    }

    #[test]
    fn settings_hide_their_button() {
        assert_eq!(visible(Some((false, true)), true, true), (false, true));
        assert_eq!(visible(Some((true, false)), true, true), (true, false));
        assert_eq!(visible(Some((false, false)), true, true), (false, false));
    }

    #[test]
    fn unsent_or_unreactable_messages_lose_buttons() {
        assert_eq!(visible(None, false, false), (false, false));
        assert_eq!(visible(None, true, false), (true, false));
    }

    #[test]
    fn span_grows_with_the_buttons() {
        assert_eq!(span(false, false, false), 34.);
        assert!(span(false, true, false) > span(false, false, false));
        assert!(span(true, true, true) > span(false, true, true));
        assert!(span(false, true, true) > span(false, true, false));
    }
}
