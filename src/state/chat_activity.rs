//! Peer activity ("typing", "recording a voice message", ...) shown in the
//! chat list row and the conversation header.
//!
//! Mirrors Telegram Desktop's `SendActionPainter::updateTexts`
//! (`history_view_send_action.cpp`): typing wins over every other action,
//! one/two/many typers get their own phrase, and groups name the sender
//! by first name while private chats show only the action. TDLib itself
//! sends `chatActionCancel` once an action expires, so no local timer is
//! kept.

use crate::lang::{Key, keys, tr, tr_with, trn};
use crate::telegram::envelope::{ChatAction, MessageSender};

/// One active action of one sender in a chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderAction {
    pub sender: MessageSender,
    pub action: ChatAction,
    /// First name (or chat title) shown in groups; empty when unknown.
    pub name: String,
}

/// Which animated glyph precedes the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityIndicator {
    /// Three dots pulsing in sequence (typing).
    Dots,
    /// A single pulsing dot (recording, uploading, choosing...).
    Pulse,
}

/// The line the chat row and header show while someone is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityLine {
    pub text: String,
    pub indicator: ActivityIndicator,
}

fn is_typing(action: ChatAction) -> bool {
    // tdesktop shows choosing a location or contact as plain typing.
    matches!(
        action,
        ChatAction::Typing | ChatAction::ChoosingLocation | ChatAction::ChoosingContact
    )
}

/// The named and bare phrases for a single non-typing action
/// (`lng_send_action_*`, `lng_user_action_*`); `None` for actions tdesktop
/// does not label.
fn action_keys(action: ChatAction) -> Option<(Key, Key)> {
    Some(match action {
        ChatAction::RecordingVideo => (
            keys::Q_CHAT_IS_RECORDING_A_VIDEO,
            keys::Q_CHAT_RECORDING_A_VIDEO,
        ),
        ChatAction::UploadingVideo => (
            keys::Q_CHAT_IS_SENDING_A_VIDEO,
            keys::Q_CHAT_SENDING_A_VIDEO,
        ),
        ChatAction::RecordingVoice => (
            keys::Q_CHAT_IS_RECORDING_A_VOICE_MESSAGE,
            keys::Q_CHAT_RECORDING_A_VOICE_MESSAGE,
        ),
        ChatAction::UploadingVoice => (
            keys::Q_CHAT_IS_SENDING_A_VOICE_MESSAGE,
            keys::Q_CHAT_SENDING_A_VOICE_MESSAGE,
        ),
        ChatAction::RecordingRound => (
            keys::Q_CHAT_IS_RECORDING_A_VIDEO_MESSAGE,
            keys::Q_CHAT_RECORDING_A_VIDEO_MESSAGE,
        ),
        ChatAction::UploadingRound => (
            keys::Q_CHAT_IS_SENDING_A_VIDEO_MESSAGE,
            keys::Q_CHAT_SENDING_A_VIDEO_MESSAGE,
        ),
        ChatAction::UploadingPhoto => (
            keys::Q_CHAT_IS_SENDING_A_PHOTO,
            keys::Q_CHAT_SENDING_A_PHOTO,
        ),
        ChatAction::UploadingDocument => {
            (keys::Q_CHAT_IS_SENDING_A_FILE, keys::Q_CHAT_SENDING_A_FILE)
        }
        ChatAction::ChoosingSticker => (
            keys::Q_CHAT_IS_CHOOSING_A_STICKER,
            keys::Q_CHAT_CHOOSING_A_STICKER,
        ),
        ChatAction::PlayingGame => (keys::Q_CHAT_IS_PLAYING_A_GAME, keys::Q_CHAT_PLAYING_A_GAME),
        _ => return None,
    })
}

/// Builds the activity line. `named` is true for groups and channels,
/// where the sender's first name prefixes the action.
pub fn activity_line(actions: &[SenderAction], named: bool) -> Option<ActivityLine> {
    let typers: Vec<&SenderAction> = actions.iter().filter(|a| is_typing(a.action)).collect();
    let name_of = |a: &SenderAction| crate::text::isolate(a.name.trim());
    if !typers.is_empty() {
        let text = match typers.as_slice() {
            [only] => {
                let name = name_of(only);
                if named && !name.is_empty() {
                    tr_with(keys::Q_CHAT_IS_TYPING, &[("name", name.as_str())])
                } else {
                    tr(keys::Q_CHAT_TYPING).to_string()
                }
            }
            [one, two] => {
                let (first, second) = (name_of(one), name_of(two));
                if first.is_empty() || second.is_empty() {
                    trn(keys::Q_CHAT_PEOPLE_ARE_TYPING, 2)
                } else {
                    tr_with(
                        keys::Q_CHAT_AND_ARE_TYPING,
                        &[("first", first.as_str()), ("second", second.as_str())],
                    )
                }
            }
            _ => trn(keys::Q_CHAT_PEOPLE_ARE_TYPING, typers.len()),
        };
        return Some(ActivityLine {
            text,
            indicator: ActivityIndicator::Dots,
        });
    }
    // Otherwise the first labelled action wins, as in tdesktop.
    actions.iter().find_map(|a| {
        let (named_key, bare_key) = action_keys(a.action)?;
        let name = name_of(a);
        let text = if named && !name.is_empty() {
            tr_with(named_key, &[("name", name.as_str())])
        } else {
            tr(bare_key).to_string()
        };
        Some(ActivityLine {
            text,
            indicator: ActivityIndicator::Pulse,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{ActivityIndicator, SenderAction, activity_line};
    use crate::telegram::envelope::{ChatAction, MessageSender};

    fn act(id: i64, action: ChatAction, name: &str) -> SenderAction {
        SenderAction {
            sender: MessageSender::User { user_id: id },
            action,
            name: name.into(),
        }
    }

    fn text(actions: &[SenderAction], named: bool) -> Option<String> {
        activity_line(actions, named).map(|l| l.text)
    }

    #[test]
    fn nothing_active_has_no_line() {
        assert_eq!(text(&[], true), None);
        assert_eq!(text(&[act(1, ChatAction::Other, "Dana")], true), None);
    }

    #[test]
    fn private_chat_shows_only_the_action() {
        assert_eq!(
            text(&[act(1, ChatAction::Typing, "Dana")], false).as_deref(),
            Some("typing")
        );
        assert_eq!(
            text(&[act(1, ChatAction::RecordingVoice, "Dana")], false).as_deref(),
            Some("recording a voice message")
        );
    }

    #[test]
    fn a_hebrew_name_stays_on_its_own_side_of_the_and() {
        let two = [
            act(1, ChatAction::Typing, "דנה"),
            act(2, ChatAction::Typing, "Eli"),
        ];
        assert_eq!(
            text(&two, true).as_deref(),
            Some("\u{2068}דנה\u{2069} and Eli are typing")
        );
    }

    #[test]
    fn group_names_one_two_and_many_typers() {
        let one = [act(1, ChatAction::Typing, "Dana")];
        assert_eq!(text(&one, true).as_deref(), Some("Dana is typing"));
        let two = [
            act(1, ChatAction::Typing, "Dana"),
            act(2, ChatAction::Typing, "Eli"),
        ];
        assert_eq!(text(&two, true).as_deref(), Some("Dana and Eli are typing"));
        let three = [
            act(1, ChatAction::Typing, "Dana"),
            act(2, ChatAction::Typing, "Eli"),
            act(3, ChatAction::Typing, "Fay"),
        ];
        assert_eq!(text(&three, true).as_deref(), Some("3 people are typing"));
    }

    #[test]
    fn unknown_names_fall_back_to_the_bare_phrase() {
        assert_eq!(
            text(&[act(1, ChatAction::Typing, "")], true).as_deref(),
            Some("typing")
        );
        assert_eq!(
            text(&[act(1, ChatAction::UploadingPhoto, " ")], true).as_deref(),
            Some("sending a photo")
        );
    }

    #[test]
    fn every_kind_has_its_tdesktop_phrase() {
        let cases = [
            (ChatAction::RecordingVideo, "recording a video"),
            (ChatAction::UploadingVideo, "sending a video"),
            (ChatAction::RecordingVoice, "recording a voice message"),
            (ChatAction::UploadingVoice, "sending a voice message"),
            (ChatAction::RecordingRound, "recording a video message"),
            (ChatAction::UploadingRound, "sending a video message"),
            (ChatAction::UploadingPhoto, "sending a photo"),
            (ChatAction::UploadingDocument, "sending a file"),
            (ChatAction::ChoosingSticker, "choosing a sticker"),
            (ChatAction::PlayingGame, "playing a game"),
        ];
        for (action, phrase) in cases {
            assert_eq!(
                text(&[act(1, action, "Dana")], false).as_deref(),
                Some(phrase)
            );
            assert_eq!(
                text(&[act(1, action, "Dana")], true),
                Some(format!("Dana is {phrase}"))
            );
            assert_eq!(
                activity_line(&[act(1, action, "Dana")], true)
                    .unwrap()
                    .indicator,
                ActivityIndicator::Pulse
            );
        }
    }

    #[test]
    fn location_and_contact_choosing_read_as_typing() {
        assert_eq!(
            text(&[act(1, ChatAction::ChoosingLocation, "Dana")], true).as_deref(),
            Some("Dana is typing")
        );
        assert_eq!(
            activity_line(&[act(1, ChatAction::ChoosingContact, "Dana")], false)
                .unwrap()
                .indicator,
            ActivityIndicator::Dots
        );
    }

    #[test]
    fn typing_wins_over_other_actions() {
        let mixed = [
            act(1, ChatAction::UploadingPhoto, "Dana"),
            act(2, ChatAction::Typing, "Eli"),
        ];
        assert_eq!(text(&mixed, true).as_deref(), Some("Eli is typing"));
    }
}
