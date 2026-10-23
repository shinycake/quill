use super::*;

fn all_on() -> ChatPermissions {
    ChatPermissions::all()
}

#[test]
fn rows_follow_tdesktop_order_and_topics_only_in_forums() {
    let plain = all_rows(false);
    assert_eq!(plain.len(), 15);
    assert_eq!(plain.first(), Some(&Permission::SendText));
    assert!(!plain.contains(&Permission::CreateTopics));
    let forum = all_rows(true);
    assert_eq!(forum.len(), 16);
    assert_eq!(
        bottom_rows(true),
        [
            Permission::AddMembers,
            Permission::CreateTopics,
            Permission::PinMessages,
            Permission::EditTag,
            Permission::ChangeInfo,
        ]
    );
}

#[test]
fn every_row_reads_and_writes_its_own_field() {
    for row in all_rows(true) {
        let mut permissions = all_on();
        row.set(&mut permissions, false);
        assert!(!row.get(&permissions), "{row:?}");
        let others_on = all_rows(true)
            .into_iter()
            .filter(|other| *other != row)
            .all(|other| other.get(&permissions));
        assert!(others_on, "{row:?} touched another field");
    }
}

#[test]
fn turning_send_messages_off_turns_embed_links_off() {
    let next = toggle(all_on(), Permission::SendText, false);
    assert!(!next.can_send_basic_messages);
    assert!(!next.can_add_link_previews);
    assert!(next.can_send_photos, "other media rows stay");
}

#[test]
fn turning_embed_links_on_turns_send_messages_on() {
    let mut start = all_on();
    start.can_send_basic_messages = false;
    start.can_add_link_previews = false;
    let next = toggle(start, Permission::EmbedLinks, true);
    assert!(next.can_add_link_previews);
    assert!(next.can_send_basic_messages);
}

#[test]
fn a_media_row_without_a_dependency_changes_alone() {
    let next = toggle(all_on(), Permission::Photos, false);
    assert!(!next.can_send_photos);
    assert_eq!(media_on(&next), 9);
    assert!(next.can_send_basic_messages);
}

#[test]
fn the_media_switch_sets_the_whole_group_and_its_dependency() {
    let mut start = all_on();
    start.can_send_basic_messages = false;
    start.can_add_link_previews = false;
    let off = set_media(all_on(), false);
    assert_eq!(media_on(&off), 0);
    assert!(
        off.can_send_basic_messages,
        "text is not in the media group"
    );
    let on = set_media(start, true);
    assert_eq!(media_on(&on), MEDIA.len());
    assert!(on.can_send_basic_messages, "embed links needs text");
}

#[test]
fn a_stored_set_is_read_strictest() {
    let mut stored = all_on();
    stored.can_send_basic_messages = false;
    let fixed = fix_dependent(stored);
    assert!(!fixed.can_add_link_previews);
    assert!(!fixed.can_send_basic_messages);
}

fn context(grants: AdminGrants) -> BoxContext {
    BoxContext {
        is_forum: true,
        is_public: false,
        is_discussion: false,
        grants,
    }
}

#[test]
fn rows_the_admin_cannot_grant_are_locked_and_start_off() {
    let grants = AdminGrants {
        change_info: false,
        pin_messages: true,
        invite_users: false,
        manage_topics: false,
    };
    let ctx = context(grants);
    assert_eq!(lock(&ctx, Permission::ChangeInfo), Some(Lock::CantEdit));
    assert_eq!(lock(&ctx, Permission::AddMembers), Some(Lock::CantEdit));
    assert_eq!(lock(&ctx, Permission::CreateTopics), Some(Lock::CantEdit));
    assert_eq!(lock(&ctx, Permission::PinMessages), None);
    assert_eq!(lock(&ctx, Permission::Photos), None);
    let start = initial(all_on(), &ctx);
    assert!(!start.can_change_info && !start.can_invite_users && !start.can_create_topics);
    assert!(start.can_pin_messages);
}

#[test]
fn public_and_discussion_groups_lock_info_and_pin() {
    let mut ctx = context(AdminGrants::OWNER);
    assert_eq!(lock(&ctx, Permission::ChangeInfo), None);
    ctx.is_discussion = true;
    assert_eq!(
        lock(&ctx, Permission::PinMessages),
        Some(Lock::DiscussionGroup)
    );
    assert!(
        initial(all_on(), &ctx).can_pin_messages,
        "a discussion group keeps the stored value"
    );
    ctx.is_public = true;
    assert_eq!(lock(&ctx, Permission::ChangeInfo), Some(Lock::PublicGroup));
    let start = initial(all_on(), &ctx);
    assert!(!start.can_change_info && !start.can_pin_messages);
    assert_eq!(
        Lock::PublicGroup.message(),
        keys::RIGHTS_PERMISSION_UNAVAILABLE
    );
}

#[test]
fn boosters_count_only_while_something_is_restricted() {
    let open = PermissionsSave::new(all_on(), 0, 3);
    assert_eq!(open.unrestrict_boost_count, 0);
    assert!(!open.needs_supergroup());
    let slow = PermissionsSave::new(all_on(), 30, 3);
    assert_eq!(slow.unrestrict_boost_count, 3);
    assert!(slow.needs_supergroup());
    let no_photos = toggle(all_on(), Permission::Photos, false);
    assert_eq!(
        PermissionsSave::new(no_photos, 0, 2).unrestrict_boost_count,
        2
    );
    // Rows outside sending (pin, info) are not send restrictions.
    let no_pin = toggle(all_on(), Permission::PinMessages, false);
    assert!(!has_send_restrictions(&no_pin, 0));
}

#[test]
fn stops_match_tdesktop() {
    assert_eq!(SLOW_MODE_STOPS, [0, 5, 10, 30, 60, 300, 900, 3600]);
    assert_eq!(BOOSTS_STOPS, [1, 2, 3, 4, 5]);
}

#[test]
fn broadcast_group_is_offered_a_thousand_below_the_limit() {
    assert!(!suggests_broadcast_group(198_999, 200_000));
    assert!(suggests_broadcast_group(199_000, 200_000));
    assert!(!suggests_broadcast_group(500_000, 0), "limit unknown");
}
