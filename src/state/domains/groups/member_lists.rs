//! The member lists of groups: the first page of each (chat, filter), the
//! next page and the lists a membership change made out of date. One
//! `MemberList` value holds how a list was asked, the request it waits for
//! and its next page, so no answer lands on a list that did not ask for it.
use crate::state::*;

type Key = (i64, MemberListFilter);

impl GroupsState {
    /// Where the first page of the list `key` stands.
    pub fn member_fetch(&self, key: Key) -> Option<&SupergroupMembersFetch> {
        self.supergroup_members.get(&key).map(|list| &list.fetch)
    }

    /// The query the member list `key` was asked with ("" for one filled
    /// elsewhere).
    pub fn member_query(&self, key: Key) -> &str {
        self.supergroup_members
            .get(&key)
            .map_or("", |list| list.query.as_str())
    }

    /// The list `key` is settled by `fetch` (a send that failed, a lookup
    /// the box gave up): its next page goes with the old page.
    pub fn set_member_fetch(&mut self, key: Key, fetch: SupergroupMembersFetch) {
        match self.supergroup_members.get_mut(&key) {
            Some(list) => {
                list.fetch = fetch;
                list.more = MoreMembers::Idle;
            }
            None => {
                self.supergroup_members
                    .insert(key, MemberList::of("", fetch));
            }
        }
    }

    /// `chat`'s members changed (a member joined, left or was changed,
    /// ownership moved): its member pages and administrators go `Stale`
    /// (a page stays drawn, a list on its first load has none yet) and ask again
    /// when something shows them.
    pub fn members_changed(&mut self, chat: i64) {
        for ((id, _), list) in self.supergroup_members.iter_mut() {
            if *id != chat {
                continue;
            }
            let placeholder = SupergroupMembersFetch::Failed(String::new());
            list.fetch = std::mem::replace(&mut list.fetch, placeholder).made_stale();
            list.more = MoreMembers::Idle;
        }
        if let Some(fetch) = self.basic_group_members.remove(&chat) {
            self.basic_group_members.insert(chat, fetch.made_stale());
        }
        if let Some(admins) = self.admin_lists.get_mut(&chat) {
            *admins = AdminListFetch::Stale;
        }
    }

    /// Whether the list `key` waits to be asked again.
    pub fn member_list_is_stale(&self, key: Key) -> bool {
        matches!(
            self.member_fetch(key),
            Some(SupergroupMembersFetch::Stale { .. })
        )
    }

    /// Whether the basic group `chat`'s member list waits to be asked again.
    pub fn basic_members_are_stale(&self, chat: i64) -> bool {
        matches!(
            self.basic_group_members.get(&chat),
            Some(SupergroupMembersFetch::Stale { .. })
        )
    }

    /// Whether `chat`'s administrators wait to be asked again.
    pub fn admin_list_is_stale(&self, chat: i64) -> bool {
        matches!(self.admin_lists.get(&chat), Some(AdminListFetch::Stale))
    }

    /// The chats whose administrators wait to be asked again.
    pub fn stale_admin_chats(&self) -> Vec<i64> {
        self.admin_lists
            .iter()
            .filter(|(_, fetch)| matches!(fetch, AdminListFetch::Stale))
            .map(|(chat, _)| *chat)
            .collect()
    }

    /// A stale basic-group member list nobody can ask again goes.
    pub(crate) fn drop_stale_basic_members(&mut self, chat: i64) {
        if self.basic_members_are_stale(chat) {
            self.basic_group_members.remove(&chat);
        }
    }

    /// A stale administrators list nobody can ask again goes.
    pub(crate) fn drop_stale_admin_list(&mut self, chat: i64) {
        if self.admin_list_is_stale(chat) {
            self.admin_lists.remove(&chat);
        }
    }

    /// A stale list nobody can ask again (no right to, not a group): it
    /// goes, so it does not wait for good.
    pub(crate) fn drop_stale_member_list(&mut self, key: Key) {
        if self.member_list_is_stale(key) {
            self.supergroup_members.remove(&key);
        }
    }

    /// A first page for `key` was sent as `request` with `query`: it
    /// replaces how the list was asked, keeps the page shown meanwhile and
    /// drops a next page of the old one.
    pub fn note_member_first(&mut self, key: Key, request: RequestId, query: &str) {
        let previous = self.supergroup_members.remove(&key);
        self.supergroup_members
            .insert(key, MemberList::asked(previous, request, query));
    }

    /// A first page for `key` with `query` could not be sent and the list
    /// had nothing to show: it is `NotSent` under that query, waiting for
    /// no request, so no older answer lands on it.
    pub(crate) fn note_member_refused(&mut self, key: Key, query: &str) {
        self.supergroup_members.insert(
            key,
            MemberList::of(
                query,
                SupergroupMembersFetch::NotSent(send_failed_line("Could not load members")),
            ),
        );
    }

    /// A first page for `key` was not sent: the list is what it was before
    /// (`None`: it was not there).
    pub(crate) fn restore_member_list(&mut self, key: Key, previous: Option<MemberList>) {
        match previous {
            Some(list) => self.supergroup_members.insert(key, list),
            None => self.supergroup_members.remove(&key),
        };
    }

    /// The first page `pending` answered: it fills the list `key` only
    /// while the list waits for exactly that request.
    pub(crate) fn answer_member_first(
        &mut self,
        key: Key,
        pending: &PendingRequest,
        members: Vec<ParsedChatMember>,
        total_count: i32,
    ) {
        if let Some(list) = self.supergroup_members.get_mut(&key)
            && list.fetch.waits_for(pending.id)
        {
            list.fetch = SupergroupMembersFetch::Loaded {
                members,
                total_count,
            };
            list.more = MoreMembers::Idle;
        }
    }

    /// The first page `pending` failed: it fails the list `key` only while
    /// the list waits for exactly that request.
    pub(crate) fn fail_member_first(&mut self, key: Key, pending: &PendingRequest, line: String) {
        if let Some(list) = self.supergroup_members.get_mut(&key)
            && list.fetch.waits_for(pending.id)
        {
            list.fetch = SupergroupMembersFetch::Failed(line);
        }
    }

    /// The next page the list `key` shows is not asked again at this
    /// offset (out, answered or failed).
    pub fn member_more_blocked(&self, key: Key) -> Option<i32> {
        self.supergroup_members
            .get(&key)
            .and_then(|list| list.more.blocked())
    }

    /// The members panel of `chat` was scrolled: a failed next page goes
    /// back to idle, so this scroll asks for it again.
    pub fn rearm_failed_member_more(&mut self, chat: i64) {
        for (_, list) in self
            .supergroup_members
            .iter_mut()
            .filter(|(key, _)| key.0 == chat)
        {
            if matches!(list.more, MoreMembers::Failed { .. }) {
                list.more = MoreMembers::Idle;
            }
        }
    }

    /// The members panel of `chat` starts over (opened): answered and failed
    /// next pages are forgotten, one still out keeps its request so its
    /// answer is matched.
    pub fn forget_member_more(&mut self, chat: i64) {
        for (_, list) in self
            .supergroup_members
            .iter_mut()
            .filter(|(key, _)| key.0 == chat)
        {
            if !matches!(list.more, MoreMembers::Asking { .. }) {
                list.more = MoreMembers::Idle;
            }
        }
    }

    /// `chat`'s failed member lists are forgotten, so a reopened panel
    /// asks for them again.
    pub fn forget_failed_member_lists(&mut self, chat: i64) {
        self.supergroup_members.retain(|(id, _), list| {
            *id != chat || !matches!(list.fetch, SupergroupMembersFetch::Failed(_))
        });
    }

    /// Sending the next page at `offset` for `key` failed (nothing is out).
    pub fn member_more_send_failed(&mut self, key: Key, offset: i32) {
        if let Some(list) = self.supergroup_members.get_mut(&key) {
            list.more = MoreMembers::Failed { offset };
        }
    }

    /// A next-page request for `key` at `offset` was sent: it extends the
    /// page the list shows.
    pub(crate) fn note_member_more(&mut self, key: Key, request: RequestId, offset: i32) {
        if let Some(list) = self.supergroup_members.get_mut(&key)
            && matches!(list.fetch, SupergroupMembersFetch::Loaded { .. })
        {
            list.more = MoreMembers::Asking { request, offset };
        }
    }

    /// The next page `pending` of `chat`'s list came back.
    pub(crate) fn more_answered(
        &mut self,
        chat: i64,
        filter: MemberListFilter,
        pending: &PendingRequest,
    ) -> bool {
        self.settle_member_more((chat, filter), pending, true)
    }

    /// Settle the next page `pending` with its outcome: whether it is the
    /// page the list `key` waits for. A page asked for an older query
    /// (another first page came since) is dropped, or it would join the
    /// new search's results.
    pub(crate) fn settle_member_more(
        &mut self,
        key: Key,
        pending: &PendingRequest,
        answered: bool,
    ) -> bool {
        let Some(list) = self.supergroup_members.get_mut(&key) else {
            return false;
        };
        let MoreMembers::Asking { request, offset } = list.more else {
            return false;
        };
        if request != pending.id {
            return false;
        }
        list.more = if answered {
            MoreMembers::Settled { offset }
        } else {
            MoreMembers::Failed { offset }
        };
        true
    }
}
