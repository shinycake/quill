//! The community ban (tdesktop `BanFromCommunityWithWarning`): the
//! member's chats in the community are asked for first; when they own
//! any, a warning lists them before the ban, otherwise (or when the ask
//! fails) the ban goes at once.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::state::fetch::Fetch;

/// What to do with a waiting ban once its member chats answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BanStep {
    Wait,
    BanNow,
    Warn(Vec<i64>),
}

/// `requestParticipantJoinedChats` answers an empty list on failure, so
/// a failed ask bans without the warning.
pub(crate) fn ban_step(fetch: &Fetch<quill::telegram::envelope::CommunityMemberChats>) -> BanStep {
    match fetch {
        Fetch::Idle | Fetch::Loading { .. } => BanStep::Wait,
        Fetch::Failed { .. } => BanStep::BanNow,
        Fetch::Ready(chats) if chats.owned_chat_ids.is_empty() => BanStep::BanNow,
        Fetch::Ready(chats) => BanStep::Warn(chats.owned_chat_ids.clone()),
    }
}

impl QuillApp {
    /// Moves a waiting ban on once its member chats answered.
    pub(super) fn drain_pending_ban(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pending) = self.admin.community.flow.pending_ban.clone() else {
            return false;
        };
        let Some(step) = self.session().map(|s| {
            ban_step(
                s.communities
                    .member_chats(pending.community_id, pending.member),
            )
        }) else {
            return false;
        };
        match step {
            BanStep::Wait => false,
            BanStep::BanNow => {
                self.admin.community.flow.pending_ban = None;
                self.send_community_ban(pending.community_id, pending.member, cx);
                true
            }
            BanStep::Warn(owned_chat_ids) => {
                self.admin.community.flow.pending_ban = None;
                self.show_community_box(
                    CommunityBox::BanWarning {
                        community_id: pending.community_id,
                        member: pending.member,
                        name: pending.name,
                        owned_chat_ids,
                    },
                    cx,
                );
                true
            }
        }
    }

    /// "Ban" on the warning: the box closes and the ban goes.
    pub(super) fn confirm_community_ban(&mut self, cx: &mut Context<Self>) {
        let Some(CommunityBox::BanWarning {
            community_id,
            member,
            ..
        }) = self.admin.community.flow.open.take()
        else {
            return;
        };
        self.send_community_ban(community_id, member, cx);
    }

    fn send_community_ban(
        &mut self,
        community_id: i64,
        member: CommunityMember,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.ban_community_member(community_id, member)
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
        }
        cx.notify();
    }

    /// The member's short name for the ban copy (`shortName`).
    pub(super) fn community_member_name(&self, member: CommunityMember) -> String {
        let session = self.session();
        match member {
            CommunityMember::User(user_id) => session
                .and_then(|s| s.user(user_id))
                .map(|user| user.first_name.clone())
                .unwrap_or_default(),
            CommunityMember::Chat(chat_id) => self.community_chat_title(chat_id),
        }
    }

    /// The community a moderated chat belongs to, when the person may ban
    /// from it (tdesktop: owner or `can_ban_members`).
    pub(in crate::ui) fn moderation_ban_community(&self, chat_id: i64) -> Option<(i64, usize)> {
        let session = self.session()?;
        let community_id = session.chat_community_id(chat_id)?;
        let community = session.groups.communities.get(&community_id)?;
        if !community.have_access || !community.can_ban_members {
            return None;
        }
        let chats = session
            .groups
            .community_full_infos
            .get(&community_id)
            .map_or(0, |info| info.chats.len());
        Some((community_id, chats))
    }

    /// Fetch the community full info a moderated chat's ban option needs
    /// (its chat count).
    pub(in crate::ui) fn want_community_for_moderation(&mut self, chat_id: i64) {
        let Some(community_id) = self.session().and_then(|s| s.chat_community_id(chat_id)) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.get_community_full_info(community_id);
        }
    }
}
