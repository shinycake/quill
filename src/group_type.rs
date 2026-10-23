//! The group and channel Type box (tdesktop
//! `boxes/peers/edit_peer_type_box.cpp`): public or private, the t.me link
//! and its availability, the additional usernames, content protection and
//! who may join or write. The box hands its choice back to the Edit box,
//! whose Save sends what changed (tdesktop `EditPeerInfoBox`
//! `saveUsernamesOrder` … `saveRequestToJoin`).
#![deny(clippy::indexing_slicing)]

use crate::lang::{Key, keys};
use crate::telegram::envelope::UsernameCheckResult;

/// tdesktop `kMinUsernameLength`.
pub const USERNAME_MIN: usize = 5;
/// tdesktop `kMaxUsernameLength`.
pub const USERNAME_MAX: usize = 32;
/// tdesktop `kUsernameCheckTimeout`: the pause after the last keystroke
/// before the server is asked.
pub const CHECK_DELAY_MS: u64 = 200;
/// The active-username limit tdesktop names in
/// `lng_usernames_activate_error` (`kMaxUsernames`).
pub const ACTIVE_USERNAMES_MAX: i64 = 10;

/// Why the typed link can't be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkProblem {
    /// Something other than a-z, 0-9 and underscores.
    BadSymbols,
    /// Fewer than [`USERNAME_MIN`] characters.
    TooShort,
    /// `USERNAME_INVALID`.
    Invalid,
    /// `USERNAME_OCCUPIED`.
    Occupied,
}

impl LinkProblem {
    /// tdesktop's line under the field.
    pub fn text(self) -> Key {
        match self {
            Self::BadSymbols => keys::CREATE_CHANNEL_LINK_BAD_SYMBOLS,
            Self::TooShort => keys::CREATE_CHANNEL_LINK_TOO_SHORT,
            Self::Invalid => keys::CREATE_CHANNEL_LINK_INVALID,
            Self::Occupied => keys::CREATE_CHANNEL_LINK_OCCUPIED,
        }
    }
}

/// What the line under the link field says (tdesktop `UsernameCheckInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkStatus {
    /// Nothing typed: no line.
    Empty,
    /// The chat's own link, untouched: no line, and Save may keep it.
    Current,
    /// "Checking name..." until the server answers.
    Pending,
    /// "This link is available".
    Available,
    Problem(LinkProblem),
    /// Taken, but sold on Fragment.
    Purchasable,
}

impl LinkStatus {
    /// tdesktop `goodUsername`: Save takes the link.
    pub fn is_good(self) -> bool {
        matches!(self, Self::Current | Self::Available)
    }
}

/// tdesktop `usernameChanged`: what the text says before the server is
/// asked. `Pending` means it is worth a check.
pub fn local_link_status(text: &str) -> LinkStatus {
    if text.is_empty() {
        return LinkStatus::Empty;
    }
    let bad = text
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'));
    if bad {
        LinkStatus::Problem(LinkProblem::BadSymbols)
    } else if text.chars().count() < USERNAME_MIN {
        LinkStatus::Problem(LinkProblem::TooShort)
    } else {
        LinkStatus::Pending
    }
}

/// What one `checkChatUsername` answer means for the box (tdesktop
/// `checkUsernameAvailability` done/fail).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    Status(LinkStatus),
    /// `CHANNELS_ADMIN_PUBLIC_TOO_MUCH`: the box goes back to private and
    /// shows the public links limit box.
    TooManyPublic,
    /// `CHANNEL_PUBLIC_GROUP_NA`: the chat can't be public; the box goes
    /// back to private.
    PublicUnavailable,
}

/// The answer for `checked`, where `editable` is the chat's own link (an
/// occupied answer for it is the chat's own).
pub fn check_outcome(result: UsernameCheckResult, checked: &str, editable: &str) -> CheckOutcome {
    match result {
        UsernameCheckResult::Available => CheckOutcome::Status(LinkStatus::Available),
        UsernameCheckResult::Occupied if checked == editable => {
            CheckOutcome::Status(LinkStatus::Available)
        }
        UsernameCheckResult::Occupied => {
            CheckOutcome::Status(LinkStatus::Problem(LinkProblem::Occupied))
        }
        UsernameCheckResult::Invalid => {
            CheckOutcome::Status(LinkStatus::Problem(LinkProblem::Invalid))
        }
        UsernameCheckResult::Purchasable => CheckOutcome::Status(LinkStatus::Purchasable),
        UsernameCheckResult::PublicChatsTooMany => CheckOutcome::TooManyPublic,
        UsernameCheckResult::PublicGroupsUnavailable => CheckOutcome::PublicUnavailable,
    }
}

/// The line under the field for `text`: the local rules first, then the
/// server's answer for exactly this text (`None` while none came).
pub fn link_status(text: &str, editable: &str, verdict: Option<CheckOutcome>) -> LinkStatus {
    match local_link_status(text) {
        LinkStatus::Pending => match verdict {
            Some(CheckOutcome::Status(status)) => status,
            Some(CheckOutcome::TooManyPublic | CheckOutcome::PublicUnavailable) => {
                LinkStatus::Empty
            }
            None if text == editable => LinkStatus::Current,
            None => LinkStatus::Pending,
        },
        local => local,
    }
}

/// What the Type box holds (tdesktop `EditPeerTypeData`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupTypeData {
    pub public: bool,
    /// The editable link, without "t.me/".
    pub username: String,
    /// The active usernames in their order.
    pub order: Vec<String>,
    /// "Restrict saving content".
    pub no_forwards: bool,
    /// "Only members" may write (discussion groups).
    pub join_to_write: bool,
    /// "Approve new members" / "Approve new subscribers".
    pub request_to_join: bool,
    /// The answer to "Apply to existing invite links", when it was asked.
    pub apply_to_invites: Option<bool>,
}

/// One request of the save, in the order tdesktop runs its stages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupTypeEdit {
    /// A basic group gets a link: it becomes a supergroup first, and the
    /// rest of the save goes to the new chat.
    Upgrade,
    /// Private again: every active username but the editable one is
    /// turned off (`disableAllSupergroupUsernames`).
    DisableAllUsernames,
    ReorderUsernames(Vec<String>),
    /// `setSupergroupUsername`; empty removes the link.
    Username(String),
    NoForwards(bool),
    JoinToSend(bool),
    JoinByRequest {
        enabled: bool,
        apply_to_invite_links: bool,
    },
}

/// The requests that bring `current` (what TDLib last reported) to
/// `wanted` (the Type box's choice). Only what changed is sent.
pub fn plan_group_type_save(
    current: &GroupTypeData,
    wanted: &GroupTypeData,
    basic_group: bool,
) -> Vec<GroupTypeEdit> {
    let mut edits = Vec::new();
    let username = if wanted.public {
        wanted.username.trim()
    } else {
        ""
    };
    if basic_group && !username.is_empty() {
        edits.push(GroupTypeEdit::Upgrade);
    }
    if wanted.public {
        if !wanted.order.is_empty() && wanted.order != current.order {
            edits.push(GroupTypeEdit::ReorderUsernames(wanted.order.clone()));
        }
    } else if current.order.iter().any(|name| *name != current.username) {
        edits.push(GroupTypeEdit::DisableAllUsernames);
    }
    if username != current.username {
        edits.push(GroupTypeEdit::Username(username.to_string()));
    }
    if wanted.no_forwards != current.no_forwards {
        edits.push(GroupTypeEdit::NoForwards(wanted.no_forwards));
    }
    if !basic_group {
        if wanted.join_to_write != current.join_to_write {
            edits.push(GroupTypeEdit::JoinToSend(wanted.join_to_write));
        }
        if wanted.request_to_join != current.request_to_join {
            edits.push(GroupTypeEdit::JoinByRequest {
                enabled: wanted.request_to_join,
                apply_to_invite_links: wanted.apply_to_invites.unwrap_or(false),
            });
        }
    }
    edits
}

/// The value of the Edit box's "Group type" / "Channel type" row.
pub fn type_row_value(public: bool, no_forwards: bool, group: bool) -> Key {
    match (public, no_forwards, group) {
        (true, _, true) => keys::MANAGE_PUBLIC_GROUP_TITLE,
        (true, _, false) => keys::MANAGE_PUBLIC_PEER_TITLE,
        (false, false, true) => keys::MANAGE_PRIVATE_GROUP_TITLE,
        (false, false, false) => keys::MANAGE_PRIVATE_PEER_TITLE,
        (false, true, true) => keys::MANAGE_PRIVATE_GROUP_NOFORWARDS_TITLE,
        (false, true, false) => keys::MANAGE_PRIVATE_PEER_NOFORWARDS_TITLE,
    }
}

/// Whether joining can need approval: anyone may join a public broadcast
/// channel (tdesktop `EditPeerTypeBox` hides "Who can send" for it).
pub fn join_approval_applies(public: bool, group: bool) -> bool {
    group || !public
}

/// tdesktop `RequestToJoinAbout`, without a guard bot.
pub fn request_to_join_about(join_to_write: bool, group: bool) -> Key {
    if !join_to_write {
        keys::MANAGE_PEER_SEND_ONLY_MEMBERS_ABOUT
    } else if group {
        keys::MANAGE_PEER_SEND_APPROVE_MEMBERS_ABOUT
    } else {
        keys::MANAGE_PEER_SEND_APPROVE_MEMBERS_ABOUT_CHANNEL
    }
}

/// A refused `setSupergroupUsername` (tdesktop `saveUsername` fail):
/// `None` when the server only said nothing changed.
pub fn username_save_problem(server_type: Option<&str>) -> Option<LinkProblem> {
    match server_type {
        Some("USERNAME_NOT_MODIFIED") => None,
        Some("USERNAME_OCCUPIED" | "USERNAMES_UNAVAILABLE") => Some(LinkProblem::Occupied),
        _ => Some(LinkProblem::Invalid),
    }
}

/// The status line of a row in the usernames list (tdesktop
/// `UsernamesList::rebuild`): the editable one can be edited in the field.
pub fn username_row_status(editable: bool, active: bool) -> Key {
    if editable {
        keys::USERNAMES_EDIT
    } else if active {
        keys::USERNAMES_ACTIVE
    } else {
        keys::USERNAMES_NON_ACTIVE
    }
}

/// Move `name` in front of `before` in the active order (a drop of the
/// dragged row onto another); `None` when either is not active.
pub fn reorder_active(order: &[String], name: &str, before: &str) -> Option<Vec<String>> {
    if name == before {
        return None;
    }
    let from = order.iter().position(|n| n == name)?;
    let to = order.iter().position(|n| n == before)?;
    let mut next = order.to_vec();
    let moved = next.remove(from);
    next.insert(to.min(next.len()), moved);
    Some(next)
}

/// The active usernames as the box shows them: the box's order, kept to
/// what TDLib still reports as active, then any it turned on since.
pub fn display_order(draft: &[String], active: &[String]) -> Vec<String> {
    let mut order: Vec<String> = draft
        .iter()
        .filter(|name| active.contains(name))
        .cloned()
        .collect();
    for name in active {
        if !order.contains(name) {
            order.push(name.clone());
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::{
        CheckOutcome, GroupTypeData, GroupTypeEdit, LinkProblem, LinkStatus, check_outcome,
        display_order, link_status, local_link_status, plan_group_type_save, reorder_active,
        username_save_problem,
    };
    use crate::telegram::envelope::UsernameCheckResult;

    fn public(username: &str, order: &[&str]) -> GroupTypeData {
        GroupTypeData {
            public: true,
            username: username.into(),
            order: order.iter().map(|n| n.to_string()).collect(),
            ..GroupTypeData::default()
        }
    }

    #[test]
    fn local_rules_come_first() {
        assert_eq!(local_link_status(""), LinkStatus::Empty);
        assert_eq!(
            local_link_status("bad-name"),
            LinkStatus::Problem(LinkProblem::BadSymbols)
        );
        assert_eq!(
            local_link_status("abcd"),
            LinkStatus::Problem(LinkProblem::TooShort)
        );
        assert_eq!(local_link_status("abc_5"), LinkStatus::Pending);
        assert_eq!(
            local_link_status("ünicode"),
            LinkStatus::Problem(LinkProblem::BadSymbols)
        );
    }

    #[test]
    fn every_check_answer_maps_to_a_state() {
        let cases = [
            (
                UsernameCheckResult::Available,
                CheckOutcome::Status(LinkStatus::Available),
            ),
            (
                UsernameCheckResult::Occupied,
                CheckOutcome::Status(LinkStatus::Problem(LinkProblem::Occupied)),
            ),
            (
                UsernameCheckResult::Invalid,
                CheckOutcome::Status(LinkStatus::Problem(LinkProblem::Invalid)),
            ),
            (
                UsernameCheckResult::Purchasable,
                CheckOutcome::Status(LinkStatus::Purchasable),
            ),
            (
                UsernameCheckResult::PublicChatsTooMany,
                CheckOutcome::TooManyPublic,
            ),
            (
                UsernameCheckResult::PublicGroupsUnavailable,
                CheckOutcome::PublicUnavailable,
            ),
        ];
        for (result, outcome) in cases {
            assert_eq!(check_outcome(result, "quillchat", "other"), outcome);
        }
        assert_eq!(
            check_outcome(UsernameCheckResult::Occupied, "mine", "mine"),
            CheckOutcome::Status(LinkStatus::Available)
        );
    }

    #[test]
    fn the_line_waits_for_the_answer_for_this_text() {
        assert_eq!(link_status("quillchat", "", None), LinkStatus::Pending);
        assert_eq!(link_status("mylink", "mylink", None), LinkStatus::Current);
        let taken = Some(CheckOutcome::Status(LinkStatus::Problem(
            LinkProblem::Occupied,
        )));
        assert_eq!(
            link_status("quillchat", "", taken),
            LinkStatus::Problem(LinkProblem::Occupied)
        );
        assert_eq!(
            link_status("ab", "", taken),
            LinkStatus::Problem(LinkProblem::TooShort)
        );
        assert!(LinkStatus::Current.is_good());
        assert!(!LinkStatus::Pending.is_good());
    }

    #[test]
    fn unchanged_type_sends_nothing() {
        let current = public("quillchat", &["quillchat", "quill"]);
        assert!(plan_group_type_save(&current, &current, false).is_empty());
    }

    #[test]
    fn going_public_sets_the_link() {
        let current = GroupTypeData::default();
        let wanted = public("newlink", &[]);
        assert_eq!(
            plan_group_type_save(&current, &wanted, false),
            vec![GroupTypeEdit::Username("newlink".into())]
        );
    }

    #[test]
    fn going_private_turns_the_links_off() {
        let current = public("quillchat", &["quillchat", "collectible"]);
        let wanted = GroupTypeData {
            public: false,
            ..current.clone()
        };
        assert_eq!(
            plan_group_type_save(&current, &wanted, false),
            vec![
                GroupTypeEdit::DisableAllUsernames,
                GroupTypeEdit::Username(String::new()),
            ]
        );
    }

    #[test]
    fn a_basic_group_upgrades_before_it_gets_a_link() {
        let current = GroupTypeData::default();
        let wanted = GroupTypeData {
            no_forwards: true,
            request_to_join: true,
            ..public("bookclub", &[])
        };
        assert_eq!(
            plan_group_type_save(&current, &wanted, true),
            vec![
                GroupTypeEdit::Upgrade,
                GroupTypeEdit::Username("bookclub".into()),
                GroupTypeEdit::NoForwards(true),
            ]
        );
    }

    #[test]
    fn toggles_and_order_follow_tdesktop_stages() {
        let current = public("quillchat", &["quillchat", "quill"]);
        let wanted = GroupTypeData {
            order: vec!["quill".into(), "quillchat".into()],
            join_to_write: true,
            request_to_join: true,
            apply_to_invites: Some(true),
            ..current.clone()
        };
        assert_eq!(
            plan_group_type_save(&current, &wanted, false),
            vec![
                GroupTypeEdit::ReorderUsernames(vec!["quill".into(), "quillchat".into()]),
                GroupTypeEdit::JoinToSend(true),
                GroupTypeEdit::JoinByRequest {
                    enabled: true,
                    apply_to_invite_links: true,
                },
            ]
        );
    }

    #[test]
    fn refused_links_read_as_tdesktop_says() {
        assert_eq!(username_save_problem(Some("USERNAME_NOT_MODIFIED")), None);
        assert_eq!(
            username_save_problem(Some("USERNAMES_UNAVAILABLE")),
            Some(LinkProblem::Occupied)
        );
        assert_eq!(username_save_problem(None), Some(LinkProblem::Invalid));
    }

    #[test]
    fn a_drop_moves_the_row_in_front_of_the_target() {
        let order: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            reorder_active(&order, "c", "a"),
            Some(vec!["c".into(), "a".into(), "b".into()])
        );
        assert_eq!(
            reorder_active(&order, "a", "c"),
            Some(vec!["b".into(), "c".into(), "a".into()])
        );
        assert_eq!(reorder_active(&order, "a", "a"), None);
        assert_eq!(reorder_active(&order, "x", "a"), None);
    }

    #[test]
    fn display_order_follows_the_draft_and_the_server() {
        let names = |list: &[&str]| list.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(
            display_order(&names(&["b", "a", "gone"]), &names(&["a", "b", "new"])),
            names(&["b", "a", "new"])
        );
        assert_eq!(display_order(&[], &names(&["a"])), names(&["a"]));
    }
}
