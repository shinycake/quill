//! What the compose field says while it is empty, and whether leaving an
//! edit needs a confirmation. Both are pure functions of the chat's state
//! (tdesktop `ComposeControls::updateFieldPlaceholder` and
//! `maybeCancelEditMessage`).

use crate::lang::{keys, tr, trn};

/// The facts the placeholder depends on, in tdesktop's priority order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaceholderFacts {
    /// An own message is being edited.
    pub editing: bool,
    /// Stars the account pays per message in this chat; 0 when free.
    pub stars_per_message: i64,
    /// The chat is a broadcast channel.
    pub broadcast: bool,
    /// Posts to the broadcast channel go out silently.
    pub silent: bool,
}

/// One of the compose field's placeholders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerPlaceholder {
    Message,
    Edit,
    Stars(i64),
    Broadcast,
    SilentBroadcast,
}

impl ComposerPlaceholder {
    /// The first case that applies: edit, Stars, broadcast, plain.
    pub fn for_facts(facts: PlaceholderFacts) -> Self {
        if facts.editing {
            Self::Edit
        } else if facts.stars_per_message > 0 {
            Self::Stars(facts.stars_per_message)
        } else if facts.broadcast && facts.silent {
            Self::SilentBroadcast
        } else if facts.broadcast {
            Self::Broadcast
        } else {
            Self::Message
        }
    }

    /// The placeholder in the interface language.
    pub fn text(self) -> String {
        match self {
            Self::Message => tr(keys::MESSAGE_PH).to_string(),
            Self::Edit => tr(keys::EDIT_MESSAGE_TEXT).to_string(),
            Self::Stars(count) => trn(keys::MESSAGE_STARS_PH, count),
            Self::Broadcast => tr(keys::BROADCAST_PH).to_string(),
            Self::SilentBroadcast => tr(keys::BROADCAST_SILENT_PH).to_string(),
        }
    }
}

/// Whether the field differs from the message being edited
/// (`EditTextChanged`): surrounding whitespace is not a change.
pub fn edit_text_changed(original_markup: &str, field_markup: &str) -> bool {
    original_markup.trim() != field_markup.trim()
}

#[cfg(test)]
mod tests {
    use super::{ComposerPlaceholder, PlaceholderFacts, edit_text_changed};

    fn facts(editing: bool, stars: i64, broadcast: bool, silent: bool) -> PlaceholderFacts {
        PlaceholderFacts {
            editing,
            stars_per_message: stars,
            broadcast,
            silent,
        }
    }

    #[test]
    fn plain_chat_asks_for_a_message() {
        assert_eq!(
            ComposerPlaceholder::for_facts(PlaceholderFacts::default()),
            ComposerPlaceholder::Message
        );
    }

    #[test]
    fn editing_beats_everything() {
        assert_eq!(
            ComposerPlaceholder::for_facts(facts(true, 5, true, true)),
            ComposerPlaceholder::Edit
        );
    }

    #[test]
    fn stars_beat_broadcast() {
        assert_eq!(
            ComposerPlaceholder::for_facts(facts(false, 5, true, true)),
            ComposerPlaceholder::Stars(5)
        );
    }

    #[test]
    fn broadcast_follows_the_silent_flag() {
        assert_eq!(
            ComposerPlaceholder::for_facts(facts(false, 0, true, false)),
            ComposerPlaceholder::Broadcast
        );
        assert_eq!(
            ComposerPlaceholder::for_facts(facts(false, 0, true, true)),
            ComposerPlaceholder::SilentBroadcast
        );
    }

    #[test]
    fn silent_alone_changes_nothing_outside_channels() {
        assert_eq!(
            ComposerPlaceholder::for_facts(facts(false, 0, false, true)),
            ComposerPlaceholder::Message
        );
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
