//! The group permissions box (tdesktop `boxes/peers/edit_peer_permissions_box.cpp`):
//! which rows it shows and in what groups, the dependency between rows, the
//! rows the viewer may not change, and what a save sends. Everything here is
//! pure so the box can be tested without a window or a session.
#![deny(clippy::indexing_slicing)]

use crate::lang::{Key, keys};
use crate::telegram::envelope::ChatPermissions;

/// One row of the box, in tdesktop's order. TDLib folds stickers, GIFs,
/// games and inline bots into `can_send_other_messages`, so tdesktop's
/// "Stickers & GIFs" row is one field here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    SendText,
    Photos,
    Videos,
    VideoMessages,
    Music,
    VoiceMessages,
    Files,
    StickersGifs,
    EmbedLinks,
    Polls,
    Reactions,
    AddMembers,
    CreateTopics,
    PinMessages,
    EditTag,
    ChangeInfo,
}

/// The "Send media" group (`lng_rights_chat_send_media`), in tdesktop's order.
pub const MEDIA: [Permission; 10] = [
    Permission::Photos,
    Permission::Videos,
    Permission::VideoMessages,
    Permission::Music,
    Permission::VoiceMessages,
    Permission::Files,
    Permission::StickersGifs,
    Permission::EmbedLinks,
    Permission::Polls,
    Permission::Reactions,
];

/// tdesktop `Dependencies(ChatRestrictions)` in TDLib terms: the first row
/// needs the second (`embed_links -> send_plain`). The other pairs there
/// join flags TDLib already keeps in one field, or point at "view
/// messages", which a default permission set always allows.
const DEPENDENCIES: [(Permission, Permission); 1] =
    [(Permission::EmbedLinks, Permission::SendText)];

impl Permission {
    pub fn get(self, permissions: &ChatPermissions) -> bool {
        match self {
            Self::SendText => permissions.can_send_basic_messages,
            Self::Photos => permissions.can_send_photos,
            Self::Videos => permissions.can_send_videos,
            Self::VideoMessages => permissions.can_send_video_notes,
            Self::Music => permissions.can_send_audios,
            Self::VoiceMessages => permissions.can_send_voice_notes,
            Self::Files => permissions.can_send_documents,
            Self::StickersGifs => permissions.can_send_other_messages,
            Self::EmbedLinks => permissions.can_add_link_previews,
            Self::Polls => permissions.can_send_polls,
            Self::Reactions => permissions.can_react_to_messages,
            Self::AddMembers => permissions.can_invite_users,
            Self::CreateTopics => permissions.can_create_topics,
            Self::PinMessages => permissions.can_pin_messages,
            Self::EditTag => permissions.can_edit_tag,
            Self::ChangeInfo => permissions.can_change_info,
        }
    }

    pub fn set(self, permissions: &mut ChatPermissions, value: bool) {
        let field = match self {
            Self::SendText => &mut permissions.can_send_basic_messages,
            Self::Photos => &mut permissions.can_send_photos,
            Self::Videos => &mut permissions.can_send_videos,
            Self::VideoMessages => &mut permissions.can_send_video_notes,
            Self::Music => &mut permissions.can_send_audios,
            Self::VoiceMessages => &mut permissions.can_send_voice_notes,
            Self::Files => &mut permissions.can_send_documents,
            Self::StickersGifs => &mut permissions.can_send_other_messages,
            Self::EmbedLinks => &mut permissions.can_add_link_previews,
            Self::Polls => &mut permissions.can_send_polls,
            Self::Reactions => &mut permissions.can_react_to_messages,
            Self::AddMembers => &mut permissions.can_invite_users,
            Self::CreateTopics => &mut permissions.can_create_topics,
            Self::PinMessages => &mut permissions.can_pin_messages,
            Self::EditTag => &mut permissions.can_edit_tag,
            Self::ChangeInfo => &mut permissions.can_change_info,
        };
        *field = value;
    }

    /// tdesktop's label (`NestedRestrictionLabelsList`).
    pub fn label(self) -> Key {
        match self {
            Self::SendText => keys::RIGHTS_CHAT_SEND_TEXT,
            Self::Photos => keys::RIGHTS_CHAT_PHOTOS,
            Self::Videos => keys::RIGHTS_CHAT_VIDEOS,
            Self::VideoMessages => keys::RIGHTS_CHAT_VIDEO_MESSAGES,
            Self::Music => keys::RIGHTS_CHAT_MUSIC,
            Self::VoiceMessages => keys::RIGHTS_CHAT_VOICE_MESSAGES,
            Self::Files => keys::RIGHTS_CHAT_FILES,
            Self::StickersGifs => keys::RIGHTS_CHAT_STICKERS,
            Self::EmbedLinks => keys::RIGHTS_CHAT_SEND_LINKS,
            Self::Polls => keys::RIGHTS_CHAT_SEND_POLLS,
            Self::Reactions => keys::RIGHTS_CHAT_SEND_REACTIONS,
            Self::AddMembers => keys::RIGHTS_CHAT_ADD_MEMBERS,
            Self::CreateTopics => keys::RIGHTS_GROUP_ADD_TOPICS,
            Self::PinMessages => keys::RIGHTS_GROUP_PIN,
            Self::EditTag => keys::RIGHTS_GROUP_EDIT_RANK,
            Self::ChangeInfo => keys::RIGHTS_GROUP_INFO,
        }
    }

    /// A short stable name, for element ids.
    pub fn id(self) -> &'static str {
        match self {
            Self::SendText => "send-text",
            Self::Photos => "photos",
            Self::Videos => "videos",
            Self::VideoMessages => "video-messages",
            Self::Music => "music",
            Self::VoiceMessages => "voice-messages",
            Self::Files => "files",
            Self::StickersGifs => "stickers-gifs",
            Self::EmbedLinks => "embed-links",
            Self::Polls => "polls",
            Self::Reactions => "reactions",
            Self::AddMembers => "add-members",
            Self::CreateTopics => "create-topics",
            Self::PinMessages => "pin-messages",
            Self::EditTag => "edit-tag",
            Self::ChangeInfo => "change-info",
        }
    }
}

/// The rows after the media group. "Create topics" is there only in a forum.
pub fn bottom_rows(is_forum: bool) -> Vec<Permission> {
    [
        Permission::AddMembers,
        Permission::CreateTopics,
        Permission::PinMessages,
        Permission::EditTag,
        Permission::ChangeInfo,
    ]
    .into_iter()
    .filter(|row| is_forum || *row != Permission::CreateTopics)
    .collect()
}

/// Every row the box shows, in order (tdesktop `ListOfRestrictions`).
pub fn all_rows(is_forum: bool) -> Vec<Permission> {
    let mut rows = vec![Permission::SendText];
    rows.extend(MEDIA);
    rows.extend(bottom_rows(is_forum));
    rows
}

/// The Edit box's "Permissions 13/14": rows on, and rows in the box
/// (tdesktop `RestrictionsCountValue`).
pub fn permissions_count(permissions: &ChatPermissions, is_forum: bool) -> (usize, usize) {
    let rows = all_rows(is_forum);
    let on = rows.iter().filter(|row| row.get(permissions)).count();
    (on, rows.len())
}

/// How many media rows are on (the "3/10" next to "Send media").
pub fn media_on(permissions: &ChatPermissions) -> usize {
    MEDIA.iter().filter(|row| row.get(permissions)).count()
}

/// tdesktop `ApplyDependencies`: while a row is on and the row it needs is
/// off, the row the user did not just change gives way. Turning "Embed
/// links" on turns "Send messages" on; turning "Send messages" off turns
/// "Embed links" off.
fn apply_dependencies(mut permissions: ChatPermissions, changed: &[Permission]) -> ChatPermissions {
    for (needs, needed) in DEPENDENCIES {
        if needs.get(&permissions) && !needed.get(&permissions) {
            if changed.contains(&needs) {
                needed.set(&mut permissions, true);
            } else {
                needs.set(&mut permissions, false);
            }
        }
    }
    permissions
}

/// Flip one row the way a click does, then settle its dependencies.
pub fn toggle(permissions: ChatPermissions, row: Permission, on: bool) -> ChatPermissions {
    let mut next = permissions;
    row.set(&mut next, on);
    apply_dependencies(next, &[row])
}

/// The switch on the "Send media" row: every media row on or off.
pub fn set_media(permissions: ChatPermissions, on: bool) -> ChatPermissions {
    let mut next = permissions;
    for row in MEDIA {
        row.set(&mut next, on);
    }
    apply_dependencies(next, &MEDIA)
}

/// tdesktop `FixDependentRestrictions`: the strictest reading of a stored
/// set, so a row is never on while the row it needs is off.
pub fn fix_dependent(permissions: ChatPermissions) -> ChatPermissions {
    apply_dependencies(permissions, &[])
}

/// The viewer's own administrator rights that decide which rows they may
/// change (tdesktop `DisabledByAdminRights`). The owner has them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdminGrants {
    pub change_info: bool,
    pub pin_messages: bool,
    pub invite_users: bool,
    pub manage_topics: bool,
}

impl AdminGrants {
    pub const OWNER: Self = Self {
        change_info: true,
        pin_messages: true,
        invite_users: true,
        manage_topics: true,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxContext {
    pub is_forum: bool,
    /// A supergroup with a public username.
    pub is_public: bool,
    /// A supergroup linked to a channel as its discussion group.
    pub is_discussion: bool,
    pub grants: AdminGrants,
}

/// Why a row cannot be changed; the box says this when it is clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lock {
    /// `lng_rights_permission_cant_edit`: the viewer lacks the admin right.
    CantEdit,
    /// `lng_rights_permission_unavailable`: public groups.
    PublicGroup,
    /// `lng_rights_permission_in_discuss`: discussion groups.
    DiscussionGroup,
}

impl Lock {
    pub fn message(self) -> Key {
        match self {
            Self::CantEdit => keys::RIGHTS_PERMISSION_CANT_EDIT,
            Self::PublicGroup => keys::RIGHTS_PERMISSION_UNAVAILABLE,
            Self::DiscussionGroup => keys::RIGHTS_PERMISSION_IN_DISCUSS,
        }
    }
}

fn disabled_by_admin(row: Permission, grants: AdminGrants) -> bool {
    match row {
        Permission::CreateTopics => !grants.manage_topics,
        Permission::PinMessages => !grants.pin_messages,
        Permission::AddMembers => !grants.invite_users,
        Permission::ChangeInfo => !grants.change_info,
        _ => false,
    }
}

pub fn lock(context: &BoxContext, row: Permission) -> Option<Lock> {
    if disabled_by_admin(row, context.grants) {
        return Some(Lock::CantEdit);
    }
    if matches!(row, Permission::ChangeInfo | Permission::PinMessages) {
        if context.is_public {
            return Some(Lock::PublicGroup);
        }
        if context.is_discussion {
            return Some(Lock::DiscussionGroup);
        }
    }
    None
}

/// The rows the box starts with: the chat's defaults, with the rows the
/// viewer may not grant shown off, and "Change info" and "Pin" off in a
/// public group (tdesktop `ShowEditPeerPermissionsBox`).
pub fn initial(defaults: ChatPermissions, context: &BoxContext) -> ChatPermissions {
    let mut permissions = defaults;
    for row in all_rows(true) {
        if disabled_by_admin(row, context.grants)
            || (context.is_public
                && matches!(row, Permission::ChangeInfo | Permission::PinMessages))
        {
            row.set(&mut permissions, false);
        }
    }
    fix_dependent(permissions)
}

/// tdesktop `kSlowmodeValues` / `SlowmodeDelayByIndex`: the delays TDLib
/// accepts, in slider order.
pub const SLOW_MODE_STOPS: [i32; 8] = [0, 5, 10, 30, 60, 300, 900, 3600];

/// tdesktop `kBoostsUnrestrictValues` / `BoostsUnrestrictByIndex`.
pub const BOOSTS_STOPS: [i32; 5] = [1, 2, 3, 4, 5];

/// tdesktop `kSendRestrictions`: any sending row off, or slow mode on,
/// is what boosters can be let past.
pub fn has_send_restrictions(permissions: &ChatPermissions, slow_mode_delay: i32) -> bool {
    slow_mode_delay > 0
        || !Permission::SendText.get(permissions)
        || MEDIA.iter().any(|row| !row.get(permissions))
}

/// What Save sends (tdesktop `EditPeerPermissionsBoxResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionsSave {
    pub permissions: ChatPermissions,
    pub slow_mode_delay: i32,
    pub unrestrict_boost_count: i32,
}

impl PermissionsSave {
    /// The boosters setting only counts while something is restricted.
    pub fn new(permissions: ChatPermissions, slow_mode_delay: i32, boosts: i32) -> Self {
        let unrestrict_boost_count = if has_send_restrictions(&permissions, slow_mode_delay) {
            boosts
        } else {
            0
        };
        Self {
            permissions,
            slow_mode_delay,
            unrestrict_boost_count,
        }
    }

    /// tdesktop migrates a basic group first when either of these is set:
    /// TDLib has them for supergroups only.
    pub fn needs_supergroup(&self) -> bool {
        self.slow_mode_delay > 0 || self.unrestrict_boost_count > 0
    }
}

/// tdesktop `AboutGigagroupCallback` threshold: the owner of a group this
/// close to `supergroup_size_max` is offered a broadcast group.
pub fn suggests_broadcast_group(member_count: i32, size_max: i32) -> bool {
    size_max > 0 && member_count >= size_max.saturating_sub(1000)
}

#[cfg(test)]
mod tests;
