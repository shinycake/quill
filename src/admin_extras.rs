//! Small admin rules and copy shared by the group and channel admin UI.
//! Mirrors tdesktop: `boxes/peers/edit_tag_control.cpp` (admin title limit),
//! lib_ui `AddLengthLimitLabel` (the counter), `edit_peer_permissions_box.cpp`
//! (broadcast group conversion), `edit_peer_info_box.cpp` (delete copy) and
//! the admin log filter in `history/admin_log/`. Pure, so it is unit tested
//! without a session or a window.

/// Longest admin title, in characters (`kRankLimit`; TDLib enforces the same
/// bound in `setChatMemberTag`).
pub const CUSTOM_TITLE_LIMIT: usize = 16;

/// The counter text lib_ui shows beside a length-limited field: nothing until
/// fewer than `min(limit / 2, 9)` characters are left, then the characters
/// left, and a minus sign once the text is over the limit.
pub fn length_counter(count: usize, limit: usize) -> Option<String> {
    let threshold = (limit / 2).min(9) as i64;
    let left = limit as i64 - count as i64;
    if left >= threshold {
        None
    } else if left < 0 {
        Some(format!("\u{2212}{}", left.abs()))
    } else {
        Some(left.to_string())
    }
}

/// Whether an admin title may be sent (empty removes it).
pub fn custom_title_fits(title: &str) -> bool {
    title.chars().count() <= CUSTOM_TITLE_LIMIT
}

/// The faded text in the empty title field: what members see by default.
pub fn custom_title_placeholder(is_owner: bool) -> &'static str {
    if is_owner { "Owner" } else { "Admin" }
}

/// Helper line under the title field (`lng_rights_edit_admin_rank_about`).
pub fn custom_title_about(is_owner: bool) -> String {
    format!(
        "A title that members will see instead of '{}'.",
        custom_title_placeholder(is_owner)
    )
}

/// First step of the broadcast group conversion (`lng_gigagroup_convert_*`).
pub const BROADCAST_FEATURES: [&str; 3] = [
    "No limit on the number of members.",
    "Only admins can send messages.",
    "Can't be turned back into a regular group.",
];

/// Second step: the permanent consequence (`lng_gigagroup_warning`).
pub const BROADCAST_WARNING: &str = "Regular members of the group (non-admins) will permanently lose their right to send messages in the group.\n\nThis action can't be undone.";

/// Line under the "Convert to broadcast group" row (`lng_rights_gigagroup_about`).
pub const BROADCAST_ABOUT: &str =
    "Broadcast groups can have over 200,000 members, but only admins can send messages in them.";

/// The bullet list shown in the first conversion dialog.
pub fn broadcast_features_text() -> String {
    BROADCAST_FEATURES
        .iter()
        .map(|line| format!("\u{2022} {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Delete confirmation copy (`lng_sure_delete_group` / `_channel`).
pub fn delete_chat_text(is_channel: bool) -> &'static str {
    if is_channel {
        "Are you sure you want to delete this channel? All subscribers will be removed and all messages will be lost."
    } else {
        "Are you sure you want to delete this group? All members will be removed, and all messages will be lost."
    }
}

/// Explainer at the top of the recent actions list
/// (`lng_admin_log_about_text[_channel]`).
pub fn event_log_about(is_channel: bool) -> &'static str {
    if is_channel {
        "This is a list of all service actions taken by the channel's admins in the last 48 hours."
    } else {
        "This is a list of all notable actions by group members and admins in the last 48 hours."
    }
}

/// Flip one admin in the server-side `user_ids` list of `getChatEventLog`.
/// Order of selection is kept; an empty list means "all admins".
pub fn toggle_event_log_user(users: &mut Vec<i64>, user_id: i64) {
    if let Some(index) = users.iter().position(|id| *id == user_id) {
        users.remove(index);
    } else {
        users.push(user_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CUSTOM_TITLE_LIMIT, broadcast_features_text, custom_title_about, custom_title_fits,
        custom_title_placeholder, delete_chat_text, event_log_about, length_counter,
        toggle_event_log_user,
    };

    #[test]
    fn counter_is_hidden_until_close_to_the_limit() {
        assert_eq!(length_counter(0, CUSTOM_TITLE_LIMIT), None);
        assert_eq!(length_counter(8, CUSTOM_TITLE_LIMIT), None);
        assert_eq!(length_counter(9, CUSTOM_TITLE_LIMIT).as_deref(), Some("7"));
        assert_eq!(length_counter(16, CUSTOM_TITLE_LIMIT).as_deref(), Some("0"));
    }

    #[test]
    fn counter_goes_negative_with_a_real_minus() {
        assert_eq!(
            length_counter(18, CUSTOM_TITLE_LIMIT).as_deref(),
            Some("\u{2212}2")
        );
    }

    #[test]
    fn counter_threshold_scales_for_short_limits() {
        // limit 10 -> threshold 5.
        assert_eq!(length_counter(5, 10), None);
        assert_eq!(length_counter(6, 10).as_deref(), Some("4"));
    }

    #[test]
    fn title_limit_counts_characters_not_bytes() {
        assert!(custom_title_fits(&"é".repeat(16)));
        assert!(!custom_title_fits(&"é".repeat(17)));
        assert!(custom_title_fits(""));
    }

    #[test]
    fn placeholder_and_about_follow_the_role() {
        assert_eq!(custom_title_placeholder(false), "Admin");
        assert_eq!(custom_title_placeholder(true), "Owner");
        assert_eq!(
            custom_title_about(false),
            "A title that members will see instead of 'Admin'."
        );
    }

    #[test]
    fn broadcast_features_are_a_bullet_list() {
        let text = broadcast_features_text();
        assert_eq!(text.lines().count(), 3);
        assert!(text.lines().all(|line| line.starts_with('\u{2022}')));
    }

    #[test]
    fn delete_and_log_copy_differ_by_kind() {
        assert!(delete_chat_text(true).contains("subscribers"));
        assert!(delete_chat_text(false).contains("members"));
        assert!(event_log_about(true).contains("channel"));
        assert!(event_log_about(false).contains("group members"));
    }

    #[test]
    fn event_log_users_toggle_keeps_order() {
        let mut users = Vec::new();
        toggle_event_log_user(&mut users, 7);
        toggle_event_log_user(&mut users, 3);
        assert_eq!(users, vec![7, 3]);
        toggle_event_log_user(&mut users, 7);
        assert_eq!(users, vec![3]);
        toggle_event_log_user(&mut users, 3);
        assert!(users.is_empty());
    }
}
