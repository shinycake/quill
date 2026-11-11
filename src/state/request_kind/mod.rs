//! What a request is, for the flood notice and for settling at logout.
//!
//! Every purpose enum classifies its own variants with an exhaustive
//! `kind()` (one file per enum here), so adding a request forces a choice
//! at compile time and a lookup nested in a wrapper (`Payments(GetGiftOption(_))`)
//! stays a lookup.
mod ai_compose_purpose;
mod auth_purpose;
mod bots_purpose;
mod calls_purpose;
mod chat_list_purpose;
mod chats_purpose;
mod communities_purpose;
mod earn_purpose;
mod gift_market_purpose;
mod grams_purpose;
mod group_type_purpose;
mod groups_purpose;
mod language_purpose;
mod live_story_purpose;
mod location_picker_purpose;
mod media_purpose;
mod messages_purpose;
mod passkeys_purpose;
mod payments_purpose;
mod search_purpose;
mod settings_purpose;
mod sticker_creator_purpose;
mod stickers_purpose;
mod stories_purpose;
mod suggested_post_purpose;
mod threads_purpose;
mod ton_wallet_purpose;
mod users_purpose;

use crate::state::RequestPurpose;

/// What a request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestKind {
    /// Something the user did on purpose (send, edit, join, close a secret
    /// chat, ...).
    UserAction,
    /// A background lookup, a view/online ping or a chat open: retried by
    /// the driver, quiet on failure.
    Lookup,
    /// A login submit, which has its own error line.
    Auth,
}

impl RequestPurpose {
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::Auth(inner) => inner.kind(),
            Self::Bots(inner) => inner.kind(),
            Self::Calls(inner) => inner.kind(),
            Self::ChatList(inner) => inner.kind(),
            Self::Chats(inner) => inner.kind(),
            Self::Communities(inner) => inner.kind(),
            Self::Groups(inner) => inner.kind(),
            Self::Media(inner) => inner.kind(),
            Self::Messages(inner) => inner.kind(),
            Self::Payments(inner) => inner.kind(),
            Self::Search(inner) => inner.kind(),
            Self::Settings(inner) => inner.kind(),
            Self::Stickers(inner) => inner.kind(),
            Self::Stories(inner) => inner.kind(),
            Self::Threads(inner) => inner.kind(),
            Self::Users(inner) => inner.kind(),
            Self::GetRepliedMessage { .. }
            | Self::GetSharedMedia { .. }
            | Self::GetAdminChatInviteLinks { .. }
            | Self::GetLinkJoinRequests { .. }
            | Self::GetChatBoosts { .. }
            | Self::GetSavedMessagesTopicHistory { .. }
            | Self::GetSavedMessagesTags { .. }
            | Self::SearchSavedMessages { .. }
            | Self::GetMessageThread { .. }
            | Self::GetMessageThreadHistory { .. }
            | Self::GetProfileChats(_) => RequestKind::Lookup,
            Self::Other => RequestKind::UserAction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RequestKind::{self, Auth, Lookup, UserAction};
    use crate::state::{PaymentsPurpose, ProfileChatsKind, RequestPurpose};

    fn kind(purpose: RequestPurpose) -> RequestKind {
        purpose.kind()
    }

    #[test]
    fn requests_are_classified_by_their_variant() {
        assert_eq!(kind(RequestPurpose::GetHistory), Lookup);
        assert_eq!(kind(RequestPurpose::OpenChat), Lookup);
        assert_eq!(kind(RequestPurpose::SetOnline), Lookup);
        assert_eq!(kind(RequestPurpose::SetPhoneNumber), Auth);
        // Closing a secret chat is what the user asked for.
        assert_eq!(kind(RequestPurpose::CloseSecretChat), UserAction);
        assert_eq!(kind(RequestPurpose::SetName), UserAction);
        assert_eq!(kind(RequestPurpose::GetPaymentForm), Lookup);
        assert_eq!(kind(RequestPurpose::SendPaymentForm), UserAction);
    }

    /// Sweep 05-200: a lookup whose variant carries a payload stays a
    /// lookup, whatever the payload is called.
    #[test]
    fn a_lookup_with_a_payload_is_not_a_user_action() {
        assert_eq!(
            kind(RequestPurpose::GetProfileChats(
                ProfileChatsKind::GroupsInCommon
            )),
            Lookup
        );
        assert_eq!(
            kind(RequestPurpose::Payments(PaymentsPurpose::GetGiftOption(
                crate::gift_manage::GiftOption::PinnedMax
            ))),
            Lookup
        );
    }
}
