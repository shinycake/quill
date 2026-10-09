//! Bot "request" keyboard buttons: which chats a bot may be offered and the
//! confirmation text. Mirrors tdesktop `boxes/peers/choose_peer_box.cpp`
//! (`ChoosePeerBoxController` filters and `MakeConfirmBox`).

use crate::telegram::envelope::{RequestChatSpec, RequestUsersSpec};

/// What a candidate chat is, for the filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerFacts {
    pub is_user: bool,
    pub is_bot: bool,
    pub is_channel: bool,
    pub is_forum: bool,
    pub has_username: bool,
    pub is_creator: bool,
    pub is_secret: bool,
}

/// Whether a private chat can be offered for `keyboardButtonTypeRequestUsers`.
/// `user_is_premium` cannot be checked (Quill does not track Premium status
/// of other users): TDLib checks it when sharing.
pub fn user_candidate(spec: &RequestUsersSpec, peer: PeerFacts) -> bool {
    if !peer.is_user || peer.is_secret {
        return false;
    }
    spec.user_is_bot.is_none_or(|want| want == peer.is_bot)
}

/// Whether a chat can be offered for `keyboardButtonTypeRequestChat`.
/// The admin-rights restrictions are checked by TDLib when sharing.
pub fn chat_candidate(spec: &RequestChatSpec, peer: PeerFacts) -> bool {
    if peer.is_user || peer.is_secret || peer.is_channel != spec.chat_is_channel {
        return false;
    }
    if spec.chat_is_forum.is_some_and(|want| want != peer.is_forum) {
        return false;
    }
    if spec
        .chat_has_username
        .is_some_and(|want| want != peer.has_username)
    {
        return false;
    }
    !spec.chat_is_created || peer.is_creator
}

/// `lng_request_peer_confirm`: "Are you sure you want to send {chat} to {bot}?"
pub fn confirm_text(names: &[String], bot: &str) -> String {
    format!(
        "Are you sure you want to send {} to {bot}?",
        names.join(", ")
    )
}

/// What the picker says when nothing qualifies.
pub fn empty_text(users: bool) -> &'static str {
    if users {
        "No chats match what the bot asks for."
    } else {
        "None of your chats match what the bot asks for."
    }
}

#[cfg(test)]
mod tests {
    use super::{PeerFacts, chat_candidate, confirm_text, user_candidate};
    use crate::telegram::envelope::{RequestChatSpec, RequestUsersSpec};

    fn user(is_bot: bool) -> PeerFacts {
        PeerFacts {
            is_user: true,
            is_bot,
            is_channel: false,
            is_forum: false,
            has_username: false,
            is_creator: false,
            is_secret: false,
        }
    }

    fn group() -> PeerFacts {
        PeerFacts {
            is_user: false,
            ..user(false)
        }
    }

    fn users(user_is_bot: Option<bool>) -> RequestUsersSpec {
        RequestUsersSpec {
            id: 1,
            user_is_bot,
            user_is_premium: None,
            max_quantity: 1,
        }
    }

    fn chat() -> RequestChatSpec {
        RequestChatSpec {
            id: 2,
            chat_is_channel: false,
            chat_is_forum: None,
            chat_has_username: None,
            chat_is_created: false,
            bot_is_member: false,
        }
    }

    #[test]
    fn users_filter_by_bot_flag_and_skip_secret_chats() {
        assert!(user_candidate(&users(None), user(true)));
        assert!(user_candidate(&users(Some(true)), user(true)));
        assert!(!user_candidate(&users(Some(true)), user(false)));
        assert!(!user_candidate(&users(Some(false)), user(true)));
        assert!(!user_candidate(&users(None), group()));
        let secret = PeerFacts {
            is_secret: true,
            ..user(false)
        };
        assert!(!user_candidate(&users(None), secret));
    }

    #[test]
    fn chats_filter_by_kind_forum_username_and_creator() {
        assert!(chat_candidate(&chat(), group()));
        assert!(!chat_candidate(&chat(), user(false)));
        let channel = PeerFacts {
            is_channel: true,
            ..group()
        };
        assert!(!chat_candidate(&chat(), channel));
        let wants_channel = RequestChatSpec {
            chat_is_channel: true,
            ..chat()
        };
        assert!(chat_candidate(&wants_channel, channel));
        let forum = RequestChatSpec {
            chat_is_forum: Some(true),
            ..chat()
        };
        assert!(!chat_candidate(&forum, group()));
        assert!(chat_candidate(
            &forum,
            PeerFacts {
                is_forum: true,
                ..group()
            }
        ));
        let public = RequestChatSpec {
            chat_has_username: Some(true),
            ..chat()
        };
        assert!(!chat_candidate(&public, group()));
        let created = RequestChatSpec {
            chat_is_created: true,
            ..chat()
        };
        assert!(!chat_candidate(&created, group()));
        assert!(chat_candidate(
            &created,
            PeerFacts {
                is_creator: true,
                ..group()
            }
        ));
    }

    #[test]
    fn confirm_names_every_peer_and_the_bot() {
        assert_eq!(
            confirm_text(&["Ann".into(), "Bo".into()], "Poll Bot"),
            "Are you sure you want to send Ann, Bo to Poll Bot?"
        );
    }
}
