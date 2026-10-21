//! What the compose field says while it is empty, and whether leaving an
//! edit needs a confirmation. Both are pure functions of the chat's state
//! (tdesktop `HistoryWidget::updateFieldPlaceholder` and
//! `maybeCancelEditMessage`).

use crate::lang::{keys, tr, tr_with, trn};

/// The topic a message to a forum supergroup goes to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopicPlaceholder {
    /// The topic's title, or the fallback for one that is not known yet.
    pub title: String,
    /// The message replies to another one.
    pub reply: bool,
}

/// The facts the placeholder depends on, in tdesktop's priority order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaceholderFacts {
    /// An own message is being edited.
    pub editing: bool,
    /// The placeholder of the bot keyboard on screen, or of the force-reply
    /// request the next message answers; empty when there is none.
    pub keyboard: String,
    /// Stars the account pays per message in this chat; 0 when free.
    pub stars_per_message: i64,
    /// The topic the message goes to, in a forum supergroup.
    pub topic: Option<TopicPlaceholder>,
    /// The chat is a broadcast channel.
    pub broadcast: bool,
    /// Posts to the broadcast channel go out silently.
    pub silent: bool,
    /// The chat is a supergroup (not a broadcast channel) where the viewer
    /// is an administrator who posts as the group.
    pub anonymous_admin: bool,
    /// The chat is a bot that makes its own topics, and none is open.
    pub bot_off_thread: bool,
}

/// One of the compose field's placeholders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerPlaceholder {
    Message,
    Edit,
    Keyboard(String),
    Stars(i64),
    Topic(TopicPlaceholder),
    Broadcast,
    SilentBroadcast,
    Anonymous,
    OffThread,
}

impl ComposerPlaceholder {
    /// The first case that applies (`HistoryWidget::updateFieldPlaceholder`):
    /// edit, the bot keyboard's own, Stars, forum topic, broadcast, anonymous
    /// admin, a bot's off-thread chat, plain.
    pub fn for_facts(facts: &PlaceholderFacts) -> Self {
        if facts.editing {
            Self::Edit
        } else if !facts.keyboard.is_empty() {
            Self::Keyboard(facts.keyboard.clone())
        } else if facts.stars_per_message > 0 {
            Self::Stars(facts.stars_per_message)
        } else if let Some(topic) = &facts.topic {
            Self::Topic(topic.clone())
        } else if facts.broadcast && facts.silent {
            Self::SilentBroadcast
        } else if facts.broadcast {
            Self::Broadcast
        } else if facts.anonymous_admin {
            Self::Anonymous
        } else if facts.bot_off_thread {
            Self::OffThread
        } else {
            Self::Message
        }
    }

    /// The placeholder in the interface language.
    pub fn text(&self) -> String {
        match self {
            Self::Message => tr(keys::MESSAGE_PH).to_string(),
            Self::Edit => tr(keys::EDIT_MESSAGE_TEXT).to_string(),
            Self::Keyboard(text) => text.clone(),
            Self::Stars(count) => trn(keys::MESSAGE_STARS_PH, *count),
            Self::Topic(topic) => {
                let key = if topic.reply {
                    keys::FORUM_REPLY_IN
                } else {
                    keys::FORUM_MESSAGE_IN
                };
                tr_with(key, &[("topic", topic.title.as_str())])
            }
            Self::Broadcast => tr(keys::BROADCAST_PH).to_string(),
            Self::SilentBroadcast => tr(keys::BROADCAST_SILENT_PH).to_string(),
            Self::Anonymous => tr(keys::SEND_ANONYMOUS_PH).to_string(),
            Self::OffThread => tr(keys::BOT_OFF_THREAD_PH).to_string(),
        }
    }
}

/// The hint an inline bot gives after `@bot ` is typed (tdesktop
/// `updateFieldPlaceholder` with `_inlineBot`, drawn after the first
/// `username.len() + 2` symbols). `text` is the field, `bot_username` the
/// resolved bot and `placeholder` its `inline_query_placeholder`. `None`
/// once anything but the `@bot` token and one separator is typed, while
/// editing, and for a bot without a placeholder.
pub fn inline_bot_hint<'a>(
    text: &str,
    editing: bool,
    bot_username: &str,
    placeholder: &'a str,
) -> Option<&'a str> {
    if editing || placeholder.is_empty() {
        return None;
    }
    // The name and at most one plain space after it (a newline or a tab is
    // not the separator tdesktop waits for).
    let typed = text.strip_prefix('@')?;
    let name = typed.strip_suffix(' ').unwrap_or(typed);
    name.eq_ignore_ascii_case(bot_username)
        .then_some(placeholder)
}

/// Whether the field differs from the message being edited
/// (`EditTextChanged`): surrounding whitespace is not a change.
pub fn edit_text_changed(original_markup: &str, field_markup: &str) -> bool {
    original_markup.trim() != field_markup.trim()
}

#[cfg(test)]
mod tests {
    use super::{ComposerPlaceholder, PlaceholderFacts, TopicPlaceholder, edit_text_changed};

    fn facts(editing: bool, stars: i64, broadcast: bool, silent: bool) -> PlaceholderFacts {
        PlaceholderFacts {
            editing,
            stars_per_message: stars,
            broadcast,
            silent,
            ..PlaceholderFacts::default()
        }
    }

    fn topic(title: &str, reply: bool) -> Option<TopicPlaceholder> {
        Some(TopicPlaceholder {
            title: title.into(),
            reply,
        })
    }

    #[test]
    fn plain_chat_asks_for_a_message() {
        assert_eq!(
            ComposerPlaceholder::for_facts(&PlaceholderFacts::default()),
            ComposerPlaceholder::Message
        );
    }

    #[test]
    fn editing_beats_everything() {
        let all = PlaceholderFacts {
            keyboard: "Pick".into(),
            topic: topic("News", false),
            bot_off_thread: true,
            ..facts(true, 5, true, true)
        };
        assert_eq!(
            ComposerPlaceholder::for_facts(&all),
            ComposerPlaceholder::Edit
        );
    }

    #[test]
    fn stars_beat_broadcast() {
        assert_eq!(
            ComposerPlaceholder::for_facts(&facts(false, 5, true, true)),
            ComposerPlaceholder::Stars(5)
        );
    }

    #[test]
    fn broadcast_follows_the_silent_flag() {
        assert_eq!(
            ComposerPlaceholder::for_facts(&facts(false, 0, true, false)),
            ComposerPlaceholder::Broadcast
        );
        assert_eq!(
            ComposerPlaceholder::for_facts(&facts(false, 0, true, true)),
            ComposerPlaceholder::SilentBroadcast
        );
    }

    #[test]
    fn silent_alone_changes_nothing_outside_channels() {
        assert_eq!(
            ComposerPlaceholder::for_facts(&facts(false, 0, false, true)),
            ComposerPlaceholder::Message
        );
    }

    #[test]
    fn an_anonymous_admin_is_asked_to_send_anonymously() {
        let anonymous = |editing, stars, broadcast| PlaceholderFacts {
            anonymous_admin: true,
            ..facts(editing, stars, broadcast, false)
        };
        assert_eq!(
            ComposerPlaceholder::for_facts(&anonymous(false, 0, false)),
            ComposerPlaceholder::Anonymous
        );
        // Editing, Stars and a broadcast channel all come first.
        assert_eq!(
            ComposerPlaceholder::for_facts(&anonymous(true, 0, false)),
            ComposerPlaceholder::Edit
        );
        assert_eq!(
            ComposerPlaceholder::for_facts(&anonymous(false, 3, false)),
            ComposerPlaceholder::Stars(3)
        );
        assert_eq!(
            ComposerPlaceholder::for_facts(&anonymous(false, 0, true)),
            ComposerPlaceholder::Broadcast
        );
        assert_eq!(ComposerPlaceholder::Anonymous.text(), "Send anonymously…");
    }

    #[test]
    fn a_bot_keyboards_own_text_beats_the_chat_ones() {
        let keyboard = PlaceholderFacts {
            keyboard: "Choose a size".into(),
            ..facts(false, 5, true, false)
        };
        let placeholder = ComposerPlaceholder::for_facts(&keyboard);
        assert_eq!(
            placeholder,
            ComposerPlaceholder::Keyboard("Choose a size".into())
        );
        assert_eq!(placeholder.text(), "Choose a size");
        // Without text the chat's own placeholder stays.
        let empty = PlaceholderFacts {
            keyboard: String::new(),
            ..keyboard
        };
        assert_eq!(
            ComposerPlaceholder::for_facts(&empty),
            ComposerPlaceholder::Stars(5)
        );
    }

    #[test]
    fn a_forum_topic_beats_the_channel_cases() {
        let forum = |reply| PlaceholderFacts {
            topic: topic("Releases", reply),
            anonymous_admin: true,
            ..PlaceholderFacts::default()
        };
        assert_eq!(
            ComposerPlaceholder::for_facts(&forum(false)).text(),
            "Message in Releases"
        );
        assert_eq!(
            ComposerPlaceholder::for_facts(&forum(true)).text(),
            "Reply in Releases"
        );
        // Stars and the keyboard come before it.
        let paid = PlaceholderFacts {
            stars_per_message: 2,
            ..forum(false)
        };
        assert_eq!(
            ComposerPlaceholder::for_facts(&paid),
            ComposerPlaceholder::Stars(2)
        );
    }

    #[test]
    fn a_bot_that_makes_its_own_topics_says_off_thread() {
        let bot = PlaceholderFacts {
            bot_off_thread: true,
            ..PlaceholderFacts::default()
        };
        let placeholder = ComposerPlaceholder::for_facts(&bot);
        assert_eq!(placeholder, ComposerPlaceholder::OffThread);
        assert_eq!(placeholder.text(), "Off-thread message");
        let paid = PlaceholderFacts {
            stars_per_message: 1,
            ..bot
        };
        assert_eq!(
            ComposerPlaceholder::for_facts(&paid),
            ComposerPlaceholder::Stars(1)
        );
    }

    #[test]
    fn an_inline_bot_hints_right_after_its_name() {
        use super::inline_bot_hint;
        let hint = |text, editing| inline_bot_hint(text, editing, "gif", "Search GIFs");
        assert_eq!(hint("@gif", false), Some("Search GIFs"));
        assert_eq!(hint("@gif ", false), Some("Search GIFs"));
        assert_eq!(hint("@GIF ", false), Some("Search GIFs"));
        // A second space, a query or an edit hide it.
        assert_eq!(hint("@gif  ", false), None);
        assert_eq!(hint("@gif\n", false), None);
        assert_eq!(hint("@gif\t", false), None);
        assert_eq!(hint("@gif cats", false), None);
        assert_eq!(hint("@gif ", true), None);
        // Another bot, no bot, or a bot without a hint.
        assert_eq!(hint("@other ", false), None);
        assert_eq!(hint("gif ", false), None);
        assert_eq!(inline_bot_hint("@gif ", false, "gif", ""), None);
    }

    #[test]
    fn stars_text_is_plural() {
        assert_eq!(ComposerPlaceholder::Stars(1).text(), "Message for 1 Star");
        assert_eq!(
            ComposerPlaceholder::Stars(25).text(),
            "Message for 25 Stars"
        );
    }

    #[test]
    fn texts_are_the_tdesktop_strings() {
        assert_eq!(ComposerPlaceholder::Edit.text(), "New message…");
        assert_eq!(
            ComposerPlaceholder::Broadcast.text(),
            "Broadcast a message…"
        );
        assert_eq!(
            ComposerPlaceholder::SilentBroadcast.text(),
            "Silent broadcast…"
        );
    }

    #[test]
    fn whitespace_is_not_an_edit() {
        assert!(!edit_text_changed("hello", "hello"));
        assert!(!edit_text_changed("hello", "  hello\n"));
        assert!(edit_text_changed("hello", "hello!"));
        assert!(edit_text_changed("hello", ""));
    }
}
