//! What a person's profile cover shows: the status line under the name
//! (tdesktop `Data::OnlineTextSpecial`, `data/data_peer_values.cpp`), the
//! Stars rating pill and the hidden last seen button
//! (`Info::Profile::TopBar`, `info_profile_top_bar.cpp`).
#![deny(clippy::indexing_slicing)]

use crate::lang::{keys, tr, trn_decimal};
use crate::telegram::envelope::{ParsedUser, UserStatusKind};

/// `kServiceNotificationsId` and the old notifications account
/// (`PeerData::isNotificationsUser`).
const NOTIFICATIONS_USER_IDS: [i64; 2] = [777_000, 333_000];

/// The status line instead of last seen, for accounts that have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialStatus {
    ServiceNotifications,
    Support,
    /// A bot with its monthly users (`userTypeBot.active_user_count`).
    BotUsers(i32),
    Bot(BotSuffix),
}

/// tdesktop `BotStatusText`: a bot whose first username ends in one of
/// these words shows it instead of "bot".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotSuffix {
    Plain,
    App,
    Agent,
    Ai,
    Game,
    NonBinary,
}

impl BotSuffix {
    pub fn of_username(username: &str) -> Self {
        let lower = username.to_lowercase();
        [
            ("app", Self::App),
            ("agent", Self::Agent),
            ("ai", Self::Ai),
            ("game", Self::Game),
            ("gay", Self::NonBinary),
        ]
        .into_iter()
        .find(|(suffix, _)| lower.ends_with(suffix))
        .map_or(Self::Plain, |(_, kind)| kind)
    }

    fn text(self) -> &'static str {
        tr(match self {
            Self::Plain => keys::STATUS_BOT,
            Self::App => keys::STATUS_BOT_APP,
            Self::Agent => keys::STATUS_BOT_AGENT,
            Self::Ai => keys::STATUS_BOT_AI,
            Self::Game => keys::STATUS_BOT_GAME,
            Self::NonBinary => keys::STATUS_BOT_GAY,
        })
    }
}

/// `PeerData::isServiceUser`: Telegram's own accounts have round ids.
fn is_service_user(user_id: i64) -> bool {
    user_id % 1000 == 0
}

impl SpecialStatus {
    /// `Data::OnlineTextSpecial`, in its order.
    pub fn of(user: &ParsedUser) -> Option<Self> {
        if NOTIFICATIONS_USER_IDS.contains(&user.id) {
            Some(Self::ServiceNotifications)
        } else if user.is_support {
            Some(Self::Support)
        } else if user.is_bot {
            Some(if user.bot_active_users > 0 {
                Self::BotUsers(user.bot_active_users)
            } else {
                Self::Bot(BotSuffix::of_username(&user.username))
            })
        } else if is_service_user(user.id) {
            Some(Self::Support)
        } else {
            None
        }
    }

    pub fn text(self) -> String {
        match self {
            Self::ServiceNotifications => tr(keys::STATUS_SERVICE_NOTIFICATIONS).to_string(),
            Self::Support => tr(keys::STATUS_SUPPORT).to_string(),
            Self::BotUsers(count) => trn_decimal(keys::BOT_STATUS_USERS, i64::from(count)),
            Self::Bot(suffix) => suffix.text().to_string(),
        }
    }
}

/// The line under a person's name: the special status, else last seen.
pub fn status_line(user: &ParsedUser) -> String {
    SpecialStatus::of(user).map_or_else(|| user.status.display(), SpecialStatus::text)
}

/// What the viewer knows about themselves that the cover depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub is_self: bool,
    pub am_premium: bool,
    pub premium_possible: bool,
}

/// tdesktop `TopBar::setupShowLastSeen`: the "when?" button shows when
/// the person's last seen is hidden only because the viewer hides theirs,
/// for a viewer who could get Premium but has not.
pub fn shows_last_seen_button(user: &ParsedUser, viewer: Viewer) -> bool {
    !viewer.is_self
        && !user.is_bot
        && !is_service_user(user.id)
        && viewer.premium_possible
        && !viewer.am_premium
        && user.status_hidden_by_me
        && user.status != UserStatusKind::Online
}

/// Which facts the cover shows, by what the profile is
/// (`TopBar` with and without `_savedMessages`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverFacts {
    /// "Saved Messages" with the bookmark avatar instead of the person.
    pub saved_messages: bool,
    /// The action tiles, the details card and the rows under them.
    pub details: bool,
    /// The Stars rating pill, when the person has a rating.
    pub rating: bool,
    /// The story ring, when the person has active stories.
    pub story_ring: bool,
    /// A click on the avatar opens the person's photos or stories.
    pub avatar_opens: bool,
}

impl CoverFacts {
    /// Your own profile is Saved Messages, as tdesktop opens it
    /// (`Info::Memento::Default` with `savedSublistsInfo`).
    pub fn for_user(is_self: bool) -> Self {
        Self {
            saved_messages: is_self,
            details: !is_self,
            rating: !is_self,
            story_ring: !is_self,
            avatar_opens: !is_self,
        }
    }
}

/// The Saved Messages status: how many chats it holds, or Loading… until
/// the count is known (tdesktop `SavedChatsCountStatus`).
pub fn saved_chats_status(topic_count: i32, loaded: bool) -> String {
    if topic_count > 0 || loaded {
        crate::lang::trn(keys::FILTERS_CHATS_COUNT, topic_count.max(0))
    } else {
        tr(keys::CONTACTS_LOADING).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: i64) -> ParsedUser {
        ParsedUser {
            id,
            first_name: "Ada".into(),
            ..Default::default()
        }
    }

    fn viewer() -> Viewer {
        Viewer {
            is_self: false,
            am_premium: false,
            premium_possible: true,
        }
    }

    #[test]
    fn bots_show_monthly_users_else_their_suffix() {
        let mut bot = user(21);
        bot.is_bot = true;
        bot.bot_active_users = 12_345;
        assert_eq!(status_line(&bot), "12,345 monthly users");
        bot.bot_active_users = 0;
        bot.username = "WeatherApp".into();
        assert_eq!(status_line(&bot), "app");
        bot.username = "helper".into();
        assert_eq!(status_line(&bot), "bot");
        bot.username = "chess_game".into();
        assert_eq!(
            SpecialStatus::of(&bot),
            Some(SpecialStatus::Bot(BotSuffix::Game))
        );
    }

    #[test]
    fn service_and_support_accounts_say_so() {
        assert_eq!(status_line(&user(777_000)), "service notifications");
        assert_eq!(status_line(&user(42_000)), "support");
        let mut support = user(31);
        support.is_support = true;
        assert_eq!(status_line(&support), "support");
    }

    #[test]
    fn people_show_last_seen() {
        let mut person = user(31);
        person.status = UserStatusKind::Recently;
        assert_eq!(SpecialStatus::of(&person), None);
        assert_eq!(status_line(&person), person.status.display());
    }

    #[test]
    fn last_seen_button_needs_a_hidden_status_and_no_premium() {
        let mut person = user(31);
        person.status = UserStatusKind::Recently;
        assert!(!shows_last_seen_button(&person, viewer()));
        person.status_hidden_by_me = true;
        assert!(shows_last_seen_button(&person, viewer()));
        let premium = Viewer {
            am_premium: true,
            ..viewer()
        };
        assert!(!shows_last_seen_button(&person, premium));
        let unavailable = Viewer {
            premium_possible: false,
            ..viewer()
        };
        assert!(!shows_last_seen_button(&person, unavailable));
        let me = Viewer {
            is_self: true,
            ..viewer()
        };
        assert!(!shows_last_seen_button(&person, me));
        person.is_bot = true;
        assert!(!shows_last_seen_button(&person, viewer()));
    }

    #[test]
    fn own_profile_is_saved_messages_without_facts() {
        let saved = CoverFacts::for_user(true);
        assert!(saved.saved_messages);
        assert!(!saved.details && !saved.rating && !saved.story_ring && !saved.avatar_opens);
        let other = CoverFacts::for_user(false);
        assert!(!other.saved_messages);
        assert!(other.details && other.rating && other.story_ring && other.avatar_opens);
    }

    #[test]
    fn saved_status_counts_chats_once_known() {
        assert_eq!(saved_chats_status(0, false), "Loading…");
        assert_eq!(saved_chats_status(0, true), "0 chats");
        assert_eq!(saved_chats_status(1, false), "1 chat");
        assert_eq!(saved_chats_status(7, false), "7 chats");
    }
}
