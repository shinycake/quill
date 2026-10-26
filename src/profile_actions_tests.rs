use super::ProfileMenuEntry as M;
use super::ProfileTile::*;
use super::{
    ManageLabel, MenuFacts, ProfilePeerKind as K, TileFacts, profile_menu_entries, profile_tiles,
};

fn tiles(kind: K) -> TileFacts {
    TileFacts {
        kind,
        joined: true,
        has_chat: true,
        can_call: false,
        has_discussion: false,
        can_manage: false,
        video_chat: false,
        can_gift: false,
        owner_or_admin: false,
        has_menu: true,
    }
}

const USER: K = K::User { is_bot: false };
const BOT: K = K::User { is_bot: true };

#[test]
fn user_gets_message_mute_call_then_more() {
    let f = TileFacts {
        can_call: true,
        can_gift: true,
        ..tiles(USER)
    };
    assert_eq!(profile_tiles(&f), [Message, Mute, Call, More]);
}

#[test]
fn user_without_calls_shows_gift_in_the_third_place() {
    let f = TileFacts {
        can_gift: true,
        ..tiles(USER)
    };
    assert_eq!(profile_tiles(&f), [Message, Mute, Gift, More]);
}

#[test]
fn bot_gets_message_mute_and_more() {
    assert_eq!(profile_tiles(&tiles(BOT)), [Message, Mute, More]);
}

#[test]
fn bot_owner_can_manage() {
    let f = TileFacts {
        can_manage: true,
        ..tiles(BOT)
    };
    assert_eq!(profile_tiles(&f), [Message, Mute, Manage, More]);
}

#[test]
fn member_of_a_group_gets_mute_and_leave() {
    assert_eq!(profile_tiles(&tiles(K::Megagroup)), [Mute, Leave, More]);
    assert_eq!(profile_tiles(&tiles(K::BasicGroup)), [Mute, Leave, More]);
}

#[test]
fn group_admin_gets_manage_and_video_chat_before_the_cap() {
    let f = TileFacts {
        can_manage: true,
        video_chat: true,
        owner_or_admin: true,
        ..tiles(K::Megagroup)
    };
    assert_eq!(profile_tiles(&f), [Mute, Manage, VideoChat, More]);
}

#[test]
fn channel_not_joined_offers_join_mute_and_report() {
    let f = TileFacts {
        joined: false,
        ..tiles(K::Channel)
    };
    assert_eq!(profile_tiles(&f), [Join, Mute, Report, More]);
}

#[test]
fn channel_subscriber_gets_discuss_gift_then_the_cap() {
    let f = TileFacts {
        has_discussion: true,
        can_gift: true,
        video_chat: true,
        ..tiles(K::Channel)
    };
    assert_eq!(profile_tiles(&f), [Mute, Discuss, LiveStream, More]);
    let f = TileFacts {
        has_discussion: true,
        can_gift: true,
        ..tiles(K::Channel)
    };
    assert_eq!(profile_tiles(&f), [Mute, Discuss, Gift, More]);
}

#[test]
fn channel_admin_who_has_not_joined_is_not_offered_report() {
    let f = TileFacts {
        joined: false,
        owner_or_admin: true,
        ..tiles(K::Channel)
    };
    assert_eq!(profile_tiles(&f), [Join, Mute, More]);
}

#[test]
fn more_needs_menu_entries_and_mute_needs_a_chat() {
    let f = TileFacts {
        has_menu: false,
        has_chat: false,
        ..tiles(USER)
    };
    assert_eq!(profile_tiles(&f), [Message]);
}

fn menu(kind: K) -> MenuFacts {
    MenuFacts {
        kind: Some(kind),
        joined: true,
        ..MenuFacts::default()
    }
}

#[test]
fn contact_menu_follows_the_tdesktop_order() {
    let f = MenuFacts {
        is_contact: true,
        has_phone: true,
        can_auto_delete: true,
        can_gift: true,
        exportable: true,
        sharing_disabled: Some(false),
        can_add_to_folder: true,
        blocked: Some(false),
        ..menu(USER)
    };
    assert_eq!(
        profile_menu_entries(&f),
        [
            M::AutoDelete,
            M::ShareContact,
            M::EditContact,
            M::SendGift,
            M::Export,
            M::ToggleSharing { disabled: false },
            M::AddToFolder,
            M::Block {
                bot: false,
                blocked: false
            },
            M::DeleteContact,
        ]
    );
}

#[test]
fn stranger_can_be_added_and_bots_cannot() {
    assert!(profile_menu_entries(&menu(USER)).contains(&M::AddContact));
    let entries = profile_menu_entries(&MenuFacts {
        sharing_disabled: Some(true),
        blocked: Some(true),
        can_manage: true,
        ..menu(BOT)
    });
    assert_eq!(
        entries,
        [
            M::Manage(ManageLabel::Bot),
            M::Block {
                bot: true,
                blocked: true
            },
        ]
    );
}

#[test]
fn own_profile_has_no_contact_entries() {
    let entries = profile_menu_entries(&MenuFacts {
        is_self: true,
        is_contact: true,
        has_phone: true,
        sharing_disabled: Some(false),
        blocked: Some(false),
        ..menu(USER)
    });
    assert_eq!(entries, [M::ShareContact]);
}

#[test]
fn channel_owner_menu() {
    let f = MenuFacts {
        is_owner: true,
        can_add_members: true,
        can_get_statistics: true,
        can_view_revenue: true,
        can_edit_stories: true,
        can_manage: true,
        can_set_personal_channel: true,
        has_discussion: true,
        direct_messages_admin: true,
        can_report: true,
        ..menu(K::Channel)
    };
    assert_eq!(
        profile_menu_entries(&f),
        [
            M::AddMembers { channel: true },
            M::Statistics,
            M::Boosts,
            M::Monetization,
            M::StoryArchive,
            M::Manage(ManageLabel::Channel),
            M::SetPersonalChannel,
            M::ViewDiscussion,
            M::DirectMessages,
            M::Leave { channel: true },
        ]
    );
}

#[test]
fn group_member_can_report_and_leave_but_basic_groups_leave_elsewhere() {
    let f = MenuFacts {
        can_report: true,
        ..menu(K::Megagroup)
    };
    assert_eq!(
        profile_menu_entries(&f),
        [M::Report, M::Leave { channel: false }]
    );
    let f = MenuFacts {
        can_report: true,
        can_add_members: true,
        ..menu(K::BasicGroup)
    };
    assert_eq!(
        profile_menu_entries(&f),
        [M::AddMembers { channel: false }, M::Report]
    );
}

#[test]
fn agent_access_sits_after_add_to_folder() {
    let entries = profile_menu_entries(&MenuFacts {
        can_add_to_folder: true,
        agent_access: true,
        ..menu(K::BasicGroup)
    });
    assert_eq!(entries, [M::AddToFolder, M::AgentAccess]);
    assert!(!profile_menu_entries(&menu(K::BasicGroup)).contains(&M::AgentAccess));
}

#[test]
fn quill_call_entries_sit_after_the_contact_entries() {
    let entries = profile_menu_entries(&MenuFacts {
        is_contact: true,
        can_video_call: true,
        can_secret_chat: true,
        ..menu(USER)
    });
    assert_eq!(
        entries,
        [
            M::EditContact,
            M::VideoCall,
            M::SecretChat,
            M::DeleteContact
        ]
    );
}
