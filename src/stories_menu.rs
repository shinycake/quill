//! Right-click menu of a story in the strip above the chat list (pure,
//! no UI).
//!
//! Telegram Desktop's `FillSourceMenu` (`dialogs/ui/dialogs_stories_content.cpp`)
//! offers, for another peer's stories: Send Message / Open Group / Open
//! Channel, View profile / group info / channel info, and "Hide Stories"
//! or "Unhide Stories" depending on which story list the peer is in. The
//! mute entry is an addition: TDLib keeps story notifications as a
//! per-chat setting (`mute_stories`), so the menu can flip it.

use crate::telegram::envelope::ChatKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryMenuAction {
    /// Open the chat.
    Open,
    /// Open the profile or group / channel info.
    Profile,
    /// Move the poster's stories to the archive list, or back.
    ToggleHidden,
    /// Mute or unmute story notifications of the chat.
    ToggleMute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryMenuItem {
    pub action: StoryMenuAction,
    pub label: &'static str,
}

pub struct StoryMenuFacts<'a> {
    pub kind: &'a ChatKind,
    /// The poster is the account itself: only the chat can be opened.
    pub is_self: bool,
    /// The poster's stories are in the archive list.
    pub hidden: bool,
    /// Story notifications of the chat are muted.
    pub muted: bool,
}

pub fn story_menu(facts: &StoryMenuFacts<'_>) -> Vec<StoryMenuItem> {
    let item = |action, label| StoryMenuItem { action, label };
    let (open, profile) = match facts.kind {
        ChatKind::Supergroup {
            is_channel: true, ..
        } => ("Open Channel", "View channel info"),
        ChatKind::Supergroup { .. } | ChatKind::BasicGroup { .. } => {
            ("Open Group", "View group info")
        }
        _ => ("Send Message", "View profile"),
    };
    let mut items = vec![item(StoryMenuAction::Open, open)];
    if facts.is_self {
        return items;
    }
    items.push(item(StoryMenuAction::Profile, profile));
    items.push(item(
        StoryMenuAction::ToggleHidden,
        if facts.hidden {
            "Unhide Stories"
        } else {
            "Hide Stories"
        },
    ));
    items.push(item(
        StoryMenuAction::ToggleMute,
        if facts.muted {
            "Unmute stories"
        } else {
            "Mute stories"
        },
    ));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::UserId;

    fn labels(kind: ChatKind, is_self: bool, hidden: bool, muted: bool) -> Vec<&'static str> {
        story_menu(&StoryMenuFacts {
            kind: &kind,
            is_self,
            hidden,
            muted,
        })
        .iter()
        .map(|i| i.label)
        .collect()
    }

    #[test]
    fn user_menu() {
        let user = ChatKind::Private { user_id: UserId(5) };
        assert_eq!(
            labels(user, false, false, false),
            [
                "Send Message",
                "View profile",
                "Hide Stories",
                "Mute stories"
            ]
        );
    }

    #[test]
    fn hidden_and_muted_flip_the_toggles() {
        let user = ChatKind::Private { user_id: UserId(5) };
        let l = labels(user, false, true, true);
        assert_eq!(l[2], "Unhide Stories");
        assert_eq!(l[3], "Unmute stories");
    }

    #[test]
    fn channel_and_group_wording() {
        let channel = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        let group = ChatKind::Supergroup {
            supergroup_id: 2,
            is_channel: false,
        };
        let basic = ChatKind::BasicGroup { basic_group_id: 3 };
        assert_eq!(
            labels(channel, false, false, false)[..2],
            ["Open Channel", "View channel info"]
        );
        assert_eq!(
            labels(group, false, false, false)[..2],
            ["Open Group", "View group info"]
        );
        assert_eq!(
            labels(basic, false, false, false)[..2],
            ["Open Group", "View group info"]
        );
    }

    #[test]
    fn own_stories_only_open_the_chat() {
        let me = ChatKind::Private { user_id: UserId(1) };
        assert_eq!(labels(me, true, false, false), ["Send Message"]);
    }
}
