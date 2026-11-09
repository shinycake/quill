//! The pages the Invite Links section of one Group Settings box opens:
//! another admin's links, one link's pending join requests and one link's
//! members. They are one value for one chat. Opening a page for another
//! chat replaces the whole value, and the box drops it when it closes or
//! moves to another chat (`QuillApp::set_group_settings_dialog`), so no
//! page of one chat can show, or be acted on, in another chat's box.
use crate::state::{AdminLinksState, InviteLinkMembersState, LinkRequestsState};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InvitePages {
    /// The chat every page below belongs to.
    chat: Option<i64>,
    admin_links: Option<AdminLinksState>,
    join_requests: Option<LinkRequestsState>,
    members: Option<InviteLinkMembersState>,
}

impl InvitePages {
    /// The chat the pages are for, if any page was opened.
    pub fn chat(&self) -> Option<i64> {
        self.chat
    }

    /// The pages of `chat`: a value for another chat is replaced whole.
    fn of(&mut self, chat: i64) -> &mut Self {
        if self.chat != Some(chat) {
            *self = Self {
                chat: Some(chat),
                ..Self::default()
            };
        }
        self
    }

    fn holds(&self, chat: i64) -> bool {
        self.chat == Some(chat)
    }

    /// Every page goes.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn admin_links(&self, chat: i64) -> Option<&AdminLinksState> {
        self.admin_links.as_ref().filter(|_| self.holds(chat))
    }

    pub fn admin_links_mut(&mut self, chat: i64) -> Option<&mut AdminLinksState> {
        if !self.holds(chat) {
            return None;
        }
        self.admin_links.as_mut()
    }

    pub fn set_admin_links(&mut self, chat: i64, state: AdminLinksState) {
        self.of(chat).admin_links = Some(state);
    }

    pub fn close_admin_links(&mut self, chat: i64) {
        if self.holds(chat) {
            self.admin_links = None;
        }
    }

    pub fn join_requests(&self, chat: i64) -> Option<&LinkRequestsState> {
        self.join_requests.as_ref().filter(|_| self.holds(chat))
    }

    pub fn join_requests_mut(&mut self, chat: i64) -> Option<&mut LinkRequestsState> {
        if !self.holds(chat) {
            return None;
        }
        self.join_requests.as_mut()
    }

    pub fn set_join_requests(&mut self, chat: i64, state: LinkRequestsState) {
        self.of(chat).join_requests = Some(state);
    }

    pub fn close_join_requests(&mut self, chat: i64) {
        if self.holds(chat) {
            self.join_requests = None;
        }
    }

    pub fn members(&self, chat: i64) -> Option<&InviteLinkMembersState> {
        self.members.as_ref().filter(|_| self.holds(chat))
    }

    pub fn members_mut(&mut self, chat: i64) -> Option<&mut InviteLinkMembersState> {
        if !self.holds(chat) {
            return None;
        }
        self.members.as_mut()
    }

    pub fn set_members(&mut self, chat: i64, state: InviteLinkMembersState) {
        self.of(chat).members = Some(state);
    }

    pub fn close_members(&mut self, chat: i64) {
        if self.holds(chat) {
            self.members = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InviteLinkFetch;

    fn links(creator: i64) -> AdminLinksState {
        AdminLinksState {
            creator_user_id: creator,
            active: InviteLinkFetch::Loading,
            revoked: InviteLinkFetch::Loading,
            active_request: None,
            revoked_request: None,
        }
    }

    #[test]
    fn a_page_for_another_chat_replaces_every_page() {
        let mut pages = InvitePages::default();
        pages.set_admin_links(1, links(7));
        assert!(pages.admin_links(1).is_some());
        assert!(pages.admin_links(2).is_none(), "another chat sees nothing");
        assert!(pages.admin_links_mut(2).is_none());
        pages.set_admin_links(2, links(8));
        assert_eq!(pages.chat(), Some(2));
        assert!(pages.admin_links(1).is_none(), "the first chat's page went");
        pages.close_admin_links(1);
        assert!(
            pages.admin_links(2).is_some(),
            "closing another chat's page is a no-op"
        );
        pages.clear();
        assert_eq!(pages, InvitePages::default());
    }
}
