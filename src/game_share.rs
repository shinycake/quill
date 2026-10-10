//! Sharing a bot game, after tdesktop's `ShowShareGameBox` (window/
//! window_peer_menu.cpp) and the game branch of `FastShareMessage`
//! (boxes/share_box.cpp): a `t.me/<bot>?game=<name>` link asks where to send
//! the game, and a game message can copy that link.

/// The shareable link of a game, `None` while the bot has no username or the
/// game no short name.
pub fn game_link(bot_username: &str, game_short_name: &str) -> Option<String> {
    let username = bot_username.trim_start_matches('@');
    (!username.is_empty() && !game_short_name.is_empty())
        .then(|| format!("https://t.me/{username}?game={game_short_name}"))
}

/// A chat as the game picker sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destination {
    /// Saved Messages (the chat with yourself).
    pub is_self: bool,
    /// A broadcast channel, where games cannot be sent.
    pub is_channel: bool,
    /// The person may send messages here.
    pub can_post: bool,
}

/// tdesktop's filter: anywhere but Saved Messages and channels, where the
/// person can send.
pub fn can_share_to(destination: Destination) -> bool {
    !destination.is_self && !destination.is_channel && destination.can_post
}

/// The confirmation line (`lng_bot_sure_share_game[_group]`).
pub fn confirm_text(chat_title: &str, is_private: bool) -> String {
    if is_private {
        format!("Share the game with {chat_title}?")
    } else {
        format!("Share the game with “{chat_title}”?")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_needs_a_username_and_a_game() {
        assert_eq!(
            game_link("chessbot", "chess").as_deref(),
            Some("https://t.me/chessbot?game=chess")
        );
        assert_eq!(
            game_link("@chessbot", "chess").as_deref(),
            Some("https://t.me/chessbot?game=chess")
        );
        assert_eq!(game_link("", "chess"), None);
        assert_eq!(game_link("chessbot", ""), None);
    }

    #[test]
    fn saved_messages_and_channels_are_not_destinations() {
        let ok = Destination {
            is_self: false,
            is_channel: false,
            can_post: true,
        };
        assert!(can_share_to(ok));
        assert!(!can_share_to(Destination {
            is_self: true,
            ..ok
        }));
        assert!(!can_share_to(Destination {
            is_channel: true,
            ..ok
        }));
        assert!(!can_share_to(Destination {
            can_post: false,
            ..ok
        }));
    }

    #[test]
    fn the_confirmation_names_the_chat() {
        assert_eq!(confirm_text("Ada", true), "Share the game with Ada?");
        assert_eq!(
            confirm_text("Chess Club", false),
            "Share the game with “Chess Club”?"
        );
    }
}
