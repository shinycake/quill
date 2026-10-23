//! The "Add exception" and "Remove user" pickers (tdesktop
//! `AddSpecialBoxController`, `boxes/peers/add_participants_box.cpp`):
//! what picking a member leads to. An admin the viewer promoted needs a
//! confirmation first; the owner and other admins cannot be picked.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::ChannelMemberStatus;

/// Which list the picker adds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialRole {
    /// Exceptions (`Role::Restricted`): the restrict box opens next.
    Restricted,
    /// Removed users (`Role::Kicked`): the member is removed for good.
    Kicked,
}

/// A question the picker asks before going on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialConfirm {
    /// `lng_sure_ban_admin`: the member is an admin the viewer may demote.
    SureBanAdmin,
    /// `lng_error_cant_ban_admin`: the owner, or an admin the viewer did
    /// not promote. Only "OK".
    CantBanAdmin,
    /// `lng_profile_sure_kick`: removing a plain member.
    Kick,
}

/// What a pick (or a confirmed question) leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialStep {
    Ask(SpecialConfirm),
    /// Open the restrict box on the member.
    Restrict,
    /// Ban the member (`ChannelData::KickedRestrictedRights`).
    Kick,
}

/// The step after picking a member with this status. `can_be_edited` is
/// `chatMemberStatusAdministrator.can_be_edited`.
pub fn pick_step(
    role: SpecialRole,
    status: ChannelMemberStatus,
    can_be_edited: bool,
) -> SpecialStep {
    let admin = match status {
        ChannelMemberStatus::Creator => return SpecialStep::Ask(SpecialConfirm::CantBanAdmin),
        ChannelMemberStatus::Administrator if !can_be_edited => {
            return SpecialStep::Ask(SpecialConfirm::CantBanAdmin);
        }
        ChannelMemberStatus::Administrator => true,
        _ => false,
    };
    match role {
        // An exception that already exists is edited directly.
        SpecialRole::Restricted if admin => SpecialStep::Ask(SpecialConfirm::SureBanAdmin),
        SpecialRole::Restricted => SpecialStep::Restrict,
        SpecialRole::Kicked if admin => SpecialStep::Ask(SpecialConfirm::SureBanAdmin),
        SpecialRole::Kicked => SpecialStep::Ask(SpecialConfirm::Kick),
    }
}

/// The step after the viewer confirmed `question`; `None` closes it.
pub fn confirmed_step(role: SpecialRole, question: SpecialConfirm) -> Option<SpecialStep> {
    match (role, question) {
        (_, SpecialConfirm::CantBanAdmin) => None,
        (SpecialRole::Restricted, _) => Some(SpecialStep::Restrict),
        (SpecialRole::Kicked, _) => Some(SpecialStep::Kick),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChannelMemberStatus as S;

    #[test]
    fn a_plain_member_is_restricted_directly_and_kicked_after_asking() {
        assert_eq!(
            pick_step(SpecialRole::Restricted, S::Member, false),
            SpecialStep::Restrict
        );
        assert_eq!(
            pick_step(SpecialRole::Restricted, S::Restricted, false),
            SpecialStep::Restrict
        );
        let ask = pick_step(SpecialRole::Kicked, S::Member, false);
        assert_eq!(ask, SpecialStep::Ask(SpecialConfirm::Kick));
        assert_eq!(
            confirmed_step(SpecialRole::Kicked, SpecialConfirm::Kick),
            Some(SpecialStep::Kick)
        );
    }

    #[test]
    fn admins_the_viewer_promoted_need_a_yes() {
        for role in [SpecialRole::Restricted, SpecialRole::Kicked] {
            assert_eq!(
                pick_step(role, S::Administrator, true),
                SpecialStep::Ask(SpecialConfirm::SureBanAdmin)
            );
        }
        assert_eq!(
            confirmed_step(SpecialRole::Restricted, SpecialConfirm::SureBanAdmin),
            Some(SpecialStep::Restrict)
        );
        // tdesktop skips the second question once the admin one is answered.
        assert_eq!(
            confirmed_step(SpecialRole::Kicked, SpecialConfirm::SureBanAdmin),
            Some(SpecialStep::Kick)
        );
    }

    #[test]
    fn the_owner_and_other_admins_cannot_be_picked() {
        for role in [SpecialRole::Restricted, SpecialRole::Kicked] {
            assert_eq!(
                pick_step(role, S::Creator, false),
                SpecialStep::Ask(SpecialConfirm::CantBanAdmin)
            );
            assert_eq!(
                pick_step(role, S::Administrator, false),
                SpecialStep::Ask(SpecialConfirm::CantBanAdmin)
            );
            assert_eq!(confirmed_step(role, SpecialConfirm::CantBanAdmin), None);
        }
    }
}
