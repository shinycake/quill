//! The buttons under a profile's name and its More menu, as Telegram
//! Desktop builds them: `TopBar::setupActions` in
//! `info/profile/info_profile_top_bar.cpp` (at most three buttons, then
//! More) and `Filler::fillProfileActions` in `window/window_peer_menu.cpp`
//! (the menu, in that order). The UI turns the session into the facts
//! below; which entries show is decided here.
#![deny(clippy::indexing_slicing)]

/// The peer a profile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilePeerKind {
    User {
        is_bot: bool,
    },
    BasicGroup,
    /// A supergroup (tdesktop `isMegagroup`).
    Megagroup,
    /// A broadcast channel (tdesktop `isBroadcast`).
    Channel,
}

impl ProfilePeerKind {
    fn is_user(self) -> bool {
        matches!(self, Self::User { .. })
    }

    fn is_bot(self) -> bool {
        matches!(self, Self::User { is_bot: true })
    }

    /// tdesktop `asChannel()`: supergroups and broadcast channels.
    fn is_channel_like(self) -> bool {
        matches!(self, Self::Megagroup | Self::Channel)
    }
}

/// One button of the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileTile {
    Message,
    Join,
    Mute,
    Call,
    Discuss,
    Manage,
    VideoChat,
    LiveStream,
    Gift,
    Report,
    Leave,
    More,
}

/// tdesktop's `chechMax`: three buttons before More.
pub const MAX_TILES: usize = 3;

/// What decides the row of another peer's profile (your own profile has
/// Quill's Edit profile button instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileFacts {
    pub kind: ProfilePeerKind,
    /// The viewer is in the group or channel (`amIn`).
    pub joined: bool,
    /// There is a chat whose notifications can be changed.
    pub has_chat: bool,
    /// Calls are not disabled for this user (`callsStatus != Disabled`).
    pub can_call: bool,
    /// A broadcast channel with a linked discussion group.
    pub has_discussion: bool,
    /// `EditPeerInfoBox::Available`.
    pub can_manage: bool,
    /// A video chat or live stream is on, or the viewer may start one.
    pub video_chat: bool,
    /// `CanSendStarGiftTo`.
    pub can_gift: bool,
    /// The viewer owns or administers the group or channel.
    pub owner_or_admin: bool,
    /// The More menu has entries.
    pub has_menu: bool,
}

pub fn profile_tiles(facts: &TileFacts) -> Vec<ProfileTile> {
    use ProfileTile::*;
    let kind = facts.kind;
    let can_join = kind.is_channel_like() && !facts.joined;
    let mut out = Vec::new();
    if kind.is_user() {
        out.push(Message);
    }
    if can_join {
        out.push(Join);
    }
    if facts.has_chat {
        out.push(Mute);
    }
    let rest = [
        (kind.is_user() && facts.can_call, Call),
        (
            kind == ProfilePeerKind::Channel && facts.has_discussion,
            Discuss,
        ),
        (facts.can_manage, Manage),
        (
            !kind.is_user() && facts.video_chat,
            if kind == ProfilePeerKind::Channel {
                LiveStream
            } else {
                VideoChat
            },
        ),
        (facts.can_gift, Gift),
        (can_join && !facts.owner_or_admin, Report),
        (!kind.is_user() && !can_join && facts.joined, Leave),
    ];
    for (shown, tile) in rest {
        if out.len() >= MAX_TILES {
            break;
        }
        if shown {
            out.push(tile);
        }
    }
    if facts.has_menu {
        out.push(More);
    }
    out
}

/// The label of the Manage entry (`addManageChat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManageLabel {
    Bot,
    Group,
    Channel,
}

/// One entry of the profile's More menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileMenuEntry {
    AutoDelete,
    AddContact,
    ShareContact,
    EditContact,
    /// Quill: a video call, which tdesktop starts from the call panel.
    VideoCall,
    /// Quill: a secret chat, which tdesktop does not have.
    SecretChat,
    /// "Add members" for groups, "Add users" for channels.
    AddMembers {
        channel: bool,
    },
    SendGift,
    Statistics,
    Boosts,
    Monetization,
    StoryArchive,
    Manage(ManageLabel),
    SetPersonalChannel,
    ViewDiscussion,
    DirectMessages,
    Export,
    /// "Disable Sharing" while sharing is on, "Enable Sharing" after.
    ToggleSharing {
        disabled: bool,
    },
    AddToFolder,
    /// Quill: the Agent access page at this chat.
    AgentAccess,
    Block {
        bot: bool,
        blocked: bool,
    },
    Report,
    Leave {
        channel: bool,
    },
    DeleteContact,
}

/// What decides the More menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuFacts {
    pub kind: Option<ProfilePeerKind>,
    pub is_self: bool,
    pub is_contact: bool,
    pub has_phone: bool,
    /// The chat's auto-delete timer may be changed (`TTLValidator::can`).
    pub can_auto_delete: bool,
    pub can_video_call: bool,
    pub can_secret_chat: bool,
    pub can_add_members: bool,
    pub can_gift: bool,
    pub can_get_statistics: bool,
    pub is_owner: bool,
    pub can_view_revenue: bool,
    pub can_edit_stories: bool,
    pub can_manage: bool,
    pub can_set_personal_channel: bool,
    pub has_discussion: bool,
    pub direct_messages_admin: bool,
    pub exportable: bool,
    /// `Some(protected)` for a private chat whose sharing can be toggled.
    pub sharing_disabled: Option<bool>,
    /// The chat is in the chat list and the account has folders.
    pub can_add_to_folder: bool,
    /// `Some(blocked)` for a user who can be blocked.
    pub blocked: Option<bool>,
    pub can_report: bool,
    pub joined: bool,
    /// Agent access is on and the profile has a chat to open it at.
    pub agent_access: bool,
}

pub fn profile_menu_entries(facts: &MenuFacts) -> Vec<ProfileMenuEntry> {
    use ProfileMenuEntry::*;
    let Some(kind) = facts.kind else {
        return Vec::new();
    };
    let user = kind.is_user() && !facts.is_self;
    let channel_like = kind.is_channel_like();
    let mut out = Vec::new();
    let mut push = |shown: bool, entry: ProfileMenuEntry| {
        if shown {
            out.push(entry);
        }
    };
    let channel = kind == ProfilePeerKind::Channel;
    push(facts.can_auto_delete, AutoDelete);
    push(user && !facts.is_contact && !kind.is_bot(), AddContact);
    push(kind.is_user() && facts.has_phone, ShareContact);
    push(user && facts.is_contact, EditContact);
    push(user && facts.can_video_call, VideoCall);
    push(user && facts.can_secret_chat, SecretChat);
    push(
        !kind.is_user() && facts.can_add_members,
        AddMembers { channel },
    );
    push(facts.can_gift, SendGift);
    push(channel_like && facts.can_get_statistics, Statistics);
    push(
        channel_like && (facts.can_get_statistics || facts.is_owner),
        Boosts,
    );
    push(channel_like && facts.can_view_revenue, Monetization);
    push(channel_like && facts.can_edit_stories, StoryArchive);
    push(
        facts.can_manage,
        Manage(match kind {
            ProfilePeerKind::User { .. } => ManageLabel::Bot,
            ProfilePeerKind::Channel => ManageLabel::Channel,
            _ => ManageLabel::Group,
        }),
    );
    push(
        channel && facts.can_set_personal_channel,
        SetPersonalChannel,
    );
    push(channel && facts.has_discussion, ViewDiscussion);
    push(channel && facts.direct_messages_admin, DirectMessages);
    push(facts.exportable, Export);
    if let Some(disabled) = facts.sharing_disabled.filter(|_| user && !kind.is_bot()) {
        push(true, ToggleSharing { disabled });
    }
    push(facts.can_add_to_folder, AddToFolder);
    push(facts.agent_access, AgentAccess);
    if let Some(blocked) = facts.blocked.filter(|_| user) {
        push(
            true,
            Block {
                bot: kind.is_bot(),
                blocked,
            },
        );
    }
    push(
        !kind.is_user() && facts.can_report && !facts.is_owner,
        Report,
    );
    push(channel_like && facts.joined, Leave { channel });
    push(user && facts.is_contact, DeleteContact);
    out
}

#[cfg(test)]
#[path = "profile_actions_tests.rs"]
mod tests;
