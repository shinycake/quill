//! Where a channel's ads sit in its history (`ChatSponsoredMessages::placed`).
use crate::ids::MessageId;

/// One ad in the history, after the message it follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SponsoredPlacement {
    /// int53 sponsored message id.
    pub ad_id: i64,
    pub after: MessageId,
}

/// How a chat's ads enter its history (tdesktop
/// `SponsoredMessages::State`), from the list's `messages_between`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SponsoredLayout {
    /// `messages_between` is 0: an ad joins the end of the history when
    /// the reader scrolls past the last message (`append`).
    AppendAtEnd,
    /// Ads go between posts, at least this many messages apart
    /// (`inject`).
    BetweenPosts(u32),
}
