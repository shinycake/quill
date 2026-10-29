//! "Add People" over a 1:1 call (tdesktop `ConfInviteController`, a
//! `ContactsBoxController` searched by `PeerListGlobalSearchController`):
//! every contact by last seen, the other side checked and disabled, and a
//! search that filters the contacts by name as you type and adds the
//! server's people below them (`contacts.search`: TDLib's
//! `searchChatsOnServer` for your own results, `searchPublicChats` for the
//! global ones). What the box shows is [`Session::add_people_view`].
//! Once the call is a conference, the people picked are invited and the
//! results are toasted ([`ConferenceInviteResults`]).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::contacts_index::{ListItem, SortMode, arrange, matches_query};
use crate::telegram::envelope::InviteGroupCallParticipantResult;

/// tdesktop `UserData::isServiceUser`: Telegram's service notifications.
const SERVICE_USER: i64 = 777_000;

/// One person in the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPeopleRow {
    pub user_id: i64,
    pub name: String,
    /// "online", "last seen …".
    pub status: String,
    /// The other side of the call: checked and disabled (tdesktop
    /// `ConfInviteRow::setAlreadyIn`).
    pub already_in: bool,
    pub checked: bool,
}

/// What the box says where it has no rows (tdesktop
/// `ContactsBoxController::checkForEmptyRows` and the search labels of
/// `PeerListContent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddPeopleEmpty {
    /// `lng_contacts_loading`: the contacts list hasn't come yet.
    ContactsLoading,
    /// `lng_contacts_not_found`: you have no contacts.
    NoContacts,
    /// `lng_contacts_loading`: nothing matched yet and the server search
    /// hasn't answered.
    SearchLoading,
    /// `lng_blocked_list_not_found`: the search found nobody.
    NoResults,
}

/// The box's list for one query.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddPeopleView {
    /// A query is typed: the "Create Call Link" row hides (tdesktop hides
    /// the list's above-widget while searching).
    pub searching: bool,
    /// The contacts (and people you picked from a search), filtered by
    /// the query.
    pub rows: Vec<AddPeopleRow>,
    /// The server's people for the query who aren't in `rows`.
    pub global: Vec<AddPeopleRow>,
    pub empty: Option<AddPeopleEmpty>,
}

impl AddPeopleView {
    /// The row Enter toggles (tdesktop `PeerListContent::submitted` and
    /// `ConfInviteController::noSearchSubmit`): the first one you can pick.
    pub fn first_pickable(&self) -> Option<i64> {
        self.rows
            .iter()
            .chain(self.global.iter())
            .find(|row| !row.already_in)
            .map(|row| row.user_id)
    }
}

impl Session {
    /// The "Add People" box for the call with `peer`: `picked` are the
    /// people checked so far, `promoted` the ones checked from a search
    /// result, latest first, which stay at the top of the list (tdesktop
    /// `prependRowFromSearchResult`).
    pub fn add_people_view(
        &self,
        peer: i64,
        picked: &[i64],
        promoted: &[i64],
        query: &str,
    ) -> AddPeopleView {
        let query = query.trim();
        let row = |user_id: i64, name: String, status: String| AddPeopleRow {
            user_id,
            name,
            status,
            already_in: user_id == peer,
            checked: user_id == peer || picked.contains(&user_id),
        };
        let contacts: Vec<ContactRow> = self
            .contact_rows()
            .into_iter()
            .filter(|contact| self.can_add_to_call(contact.user_id))
            .collect();
        let mut rows: Vec<AddPeopleRow> = promoted
            .iter()
            .filter(|user_id| !contacts.iter().any(|c| c.user_id == **user_id))
            .filter_map(|user_id| self.users.get(user_id))
            .filter(|user| self.can_add_to_call(user.id))
            .map(|user| (user.id, user.display_name(), user.status.display()))
            .filter(|(user_id, name, _)| self.person_matches(*user_id, name, query))
            .map(|(user_id, name, status)| row(user_id, name, status))
            .collect();
        let matching: Vec<ContactRow> = contacts
            .into_iter()
            .filter(|contact| self.person_matches(contact.user_id, &contact.name, query))
            .collect();
        rows.extend(
            arrange(&matching, SortMode::Online, "")
                .into_iter()
                .filter_map(|item| match item {
                    ListItem::Contact(contact) => Some(contact),
                    ListItem::Header(_) => None,
                })
                .map(|contact| row(contact.user_id, contact.name, contact.status_text)),
        );
        let search = &self.calls.add_people_search;
        let mut global = Vec::new();
        if !query.is_empty() && search.query == query {
            for chat_id in search.local.iter().chain(search.server.iter()) {
                let Some(user) = self
                    .private_chat_user_id(ChatId(*chat_id))
                    .and_then(|user_id| self.users.get(&user_id))
                else {
                    continue;
                };
                let shown = |rows: &[AddPeopleRow]| rows.iter().any(|r| r.user_id == user.id);
                if self.can_add_to_call(user.id) && !shown(&rows) && !shown(&global) {
                    global.push(row(user.id, user.display_name(), user.status.display()));
                }
            }
        }
        let empty = match (rows.is_empty() && global.is_empty(), query.is_empty()) {
            (false, _) => None,
            (true, true) if self.users_state.contacts.is_some() => Some(AddPeopleEmpty::NoContacts),
            (true, true) => Some(AddPeopleEmpty::ContactsLoading),
            (true, false) if search.query == query && search.is_searching() => {
                Some(AddPeopleEmpty::SearchLoading)
            }
            (true, false) => Some(AddPeopleEmpty::NoResults),
        };
        AddPeopleView {
            searching: !query.is_empty(),
            rows,
            global,
            empty,
        }
    }

    /// The peer list's search (tdesktop `PeerData::fillNames`): every
    /// word of the query starts a word of the name or the username.
    fn person_matches(&self, user_id: i64, name: &str, query: &str) -> bool {
        let username = self
            .users
            .get(&user_id)
            .map(|user| user.username.as_str())
            .unwrap_or_default();
        matches_query(&format!("{name} {username}"), query)
    }

    /// tdesktop `ConfInviteController::createRow`: not you, a bot,
    /// Telegram's service account or a deleted account.
    fn can_add_to_call(&self, user_id: i64) -> bool {
        Some(user_id) != self.my_user_id
            && user_id != SERVICE_USER
            && self
                .users
                .get(&user_id)
                .is_some_and(|user| !user.is_bot && !user.is_deleted)
    }
}

/// The invitations sent once a call became a conference, and how each
/// went (tdesktop `InviteResult`, toasted by `Panel::migrationInviteUsers`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConferenceInviteResults {
    /// Sent and not answered yet: the request and whom it invites.
    pending: Vec<(RequestId, i64)>,
    pub invited: Vec<i64>,
    pub already_in: Vec<i64>,
    pub privacy_restricted: Vec<i64>,
    pub kicked: Vec<i64>,
    pub failed: Vec<i64>,
}

impl ConferenceInviteResults {
    /// An invitation went out.
    pub(crate) fn sent(&mut self, request: RequestId, user_id: i64) {
        self.pending.push((request, user_id));
    }

    /// Whether `request` is one of the invitations still unanswered.
    pub(crate) fn owns(&self, request: Option<RequestId>) -> bool {
        request.is_some_and(|request| self.pending.iter().any(|(id, _)| *id == request))
    }

    /// The answer to one invitation; `None` is a failed request.
    pub(crate) fn answered(
        &mut self,
        request: RequestId,
        result: Option<InviteGroupCallParticipantResult>,
    ) {
        let Some(at) = self.pending.iter().position(|(id, _)| *id == request) else {
            return;
        };
        let (_, user_id) = self.pending.remove(at);
        let bucket = match result {
            Some(InviteGroupCallParticipantResult::Success { .. }) => &mut self.invited,
            Some(InviteGroupCallParticipantResult::UserAlreadyParticipant) => &mut self.already_in,
            Some(InviteGroupCallParticipantResult::UserPrivacyRestricted) => {
                &mut self.privacy_restricted
            }
            Some(InviteGroupCallParticipantResult::UserWasBanned) => &mut self.kicked,
            None => &mut self.failed,
        };
        bucket.push(user_id);
    }

    /// Every invitation has its answer: the toast can show.
    pub fn is_done(&self) -> bool {
        self.pending.is_empty()
    }
}

/// The toast for the invitations' results (tdesktop
/// `ComposeInviteResultToast`): a paragraph per outcome, a name when one
/// person had it, else a count; `**` marks the bold. tdesktop words a
/// privacy refusal as a failure. `first_name` names a person.
pub fn conference_invite_toast(
    results: &ConferenceInviteResults,
    first_name: impl Fn(i64) -> String,
) -> String {
    use crate::lang::{keys, tr_with, trn_with};
    let lines = [
        (
            &results.invited,
            keys::CONFCALL_INVITE_DONE_USER,
            keys::CONFCALL_INVITE_DONE_MANY,
        ),
        (
            &results.already_in,
            keys::CONFCALL_INVITE_ALREADY_USER,
            keys::CONFCALL_INVITE_ALREADY_MANY,
        ),
        (
            &results.privacy_restricted,
            keys::CONFCALL_INVITE_FAIL_USER,
            keys::CONFCALL_INVITE_FAIL_MANY,
        ),
        (
            &results.kicked,
            keys::CONFCALL_INVITE_KICKED_USER,
            keys::CONFCALL_INVITE_KICKED_MANY,
        ),
        (
            &results.failed,
            keys::CONFCALL_INVITE_FAIL_USER,
            keys::CONFCALL_INVITE_FAIL_MANY,
        ),
    ];
    let mut parts = Vec::new();
    for (users, one, many) in lines {
        match users.as_slice() {
            [] => {}
            [user_id] => {
                let name = format!("**{}**", first_name(*user_id));
                parts.push(tr_with(one, &[("user", &name)]));
            }
            users => parts.push(trn_with(many, users.len(), &[])),
        }
    }
    parts.join("\n\n")
}

#[cfg(test)]
#[path = "conference_add_people_tests.rs"]
mod tests;
