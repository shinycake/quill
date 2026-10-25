//! Right-click menu of a story in the strip above the chat list (pure,
//! no UI).
//!
//! Telegram Desktop's `FillSourceMenu` (`dialogs/ui/dialogs_stories_content.cpp`)
//! offers, for another peer's stories: Send Message / Open Group / Open
//! Channel, View profile / group info / channel info, and "Hide Stories"
//! or "Unhide Stories" depending on which story list the peer is in. The
//! mute entry is an addition: TDLib keeps story notifications as a
//! per-chat setting (`mute_stories`), so the menu can flip it.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::ChatKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryMenuAction {
    /// Open the chat.
    Open,
    /// Open the profile or group / channel info.
    Profile,
    /// The Stealth Mode box, which opens the stories once it is on;
    /// `locked` marks the entry for an account that cannot turn it on.
    ViewAnonymously { locked: bool },
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
    pub stealth: Option<crate::story_stealth::StealthEntry>,
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
    if let Some(entry) = facts.stealth {
        items.push(item(
            StoryMenuAction::ViewAnonymously {
                locked: entry.locked,
            },
            crate::lang::tr(crate::lang::keys::STORIES_VIEW_ANONYMOUSLY),
        ));
    }
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
            stealth: None,
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
    fn unread_stories_of_a_user_offer_view_anonymously() {
        let user = ChatKind::Private { user_id: UserId(5) };
        let items = story_menu(&StoryMenuFacts {
            kind: &user,
            is_self: false,
            hidden: false,
            muted: false,
            stealth: Some(crate::story_stealth::StealthEntry { locked: true }),
        });
        let labels: Vec<_> = items.iter().map(|i| i.label).collect();
        assert_eq!(
            labels,
            [
                "Send Message",
                "View profile",
                "View Anonymously",
                "Hide Stories",
                "Mute stories"
            ]
        );
        assert_eq!(
            items.get(2).map(|i| i.action),
            Some(StoryMenuAction::ViewAnonymously { locked: true })
        );
    }

    #[test]
    fn own_stories_only_open_the_chat() {
        let me = ChatKind::Private { user_id: UserId(1) };
        assert_eq!(labels(me, true, false, false), ["Send Message"]);
    }
}
