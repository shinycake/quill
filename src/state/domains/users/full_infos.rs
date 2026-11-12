//! The per-user `getUserFullInfo` cache: one [`Fetch`] per user, so what was
//! answered and that the last ask failed are one value, not a map and a set
//! kept in step.
use crate::state::UserFullInfoData;
use crate::state::fetch::Fetch;
use std::collections::HashMap;

/// Full infos by user id. `get` shows the data of an answered (or, after a
/// failure, last answered) ask; a failed ask is remembered so a render's
/// `ensure_user_full_info` does not repeat it.
#[derive(Debug, Default)]
pub struct UserFullInfos {
    by_user: HashMap<i64, Fetch<UserFullInfoData>>,
}

impl UserFullInfos {
    pub fn get(&self, user_id: &i64) -> Option<&UserFullInfoData> {
        self.by_user.get(user_id)?.data()
    }

    pub fn get_mut(&mut self, user_id: &i64) -> Option<&mut UserFullInfoData> {
        self.by_user.get_mut(user_id)?.data_mut()
    }

    pub fn contains_key(&self, user_id: &i64) -> bool {
        self.get(user_id).is_some()
    }

    /// A pushed update (`updateUserFullInfo`) or a seeded value: the data,
    /// and no failure. An ask still out keeps waiting for its own answer,
    /// with this as the data shown meanwhile.
    pub fn insert(&mut self, user_id: i64, data: UserFullInfoData) {
        match self.by_user.get_mut(&user_id) {
            Some(Fetch::Loading { stale, .. }) => *stale = Some(data),
            _ => {
                self.by_user.insert(user_id, Fetch::Ready(data));
            }
        }
    }

    /// The answer of `request` (`None`: it names none). The ask that waits
    /// for it is done; one that waits for another request, started since,
    /// is not, and shows this meanwhile.
    pub fn answered(
        &mut self,
        user_id: i64,
        request: Option<crate::ids::RequestId>,
        data: UserFullInfoData,
    ) {
        match self.by_user.get_mut(&user_id) {
            Some(entry) if entry.is_loading() && !request.is_some_and(|r| entry.waits_for(r)) => {
                if let Fetch::Loading { stale, .. } = entry {
                    *stale = Some(data);
                }
            }
            _ => {
                self.by_user.insert(user_id, Fetch::Ready(data));
            }
        }
    }

    pub fn remove(&mut self, user_id: &i64) {
        self.by_user.remove(user_id);
    }

    /// This user's full info is needed and not cached: an idle entry the
    /// driver's pump turns into a `getUserFullInfo`, which an answer
    /// replaces and a failure turns into `Failed`.
    pub fn want(&mut self, user_id: i64) {
        self.by_user.entry(user_id).or_default();
    }

    /// Whether [`Self::want`] is still waiting for its answer: not asked
    /// yet, or asked and not answered.
    pub fn is_wanted(&self, user_id: &i64) -> bool {
        self.by_user
            .get(user_id)
            .is_some_and(|entry| entry.is_idle() || entry.is_loading())
    }

    /// Users that are wanted and not yet asked for or answered.
    pub fn wanted(&self) -> Vec<i64> {
        self.by_user
            .iter()
            .filter(|(_, entry)| entry.is_idle())
            .map(|(id, _)| *id)
            .collect()
    }

    /// The ask for this user could not be sent: it fails, and the
    /// entry's pump does not repeat it.
    pub fn fail_unsent(&mut self, user_id: i64, error: String) {
        self.by_user.entry(user_id).or_default().fail(error);
    }

    /// `request` failed. Only the ask that waits for it is affected (an
    /// older request's late error leaves a newer ask waiting): a refresh
    /// of a user whose info is held goes back to showing it, and a first
    /// ask fails with the reason.
    pub fn failed_request(
        &mut self,
        user_id: i64,
        request: Option<crate::ids::RequestId>,
        error: String,
    ) {
        let Some(entry) = self.by_user.get_mut(&user_id) else {
            return;
        };
        if !request.is_some_and(|request| entry.waits_for(request)) {
            return;
        }
        if entry.data().is_some() {
            entry.cancel();
        } else {
            entry.fail(error);
        }
    }

    /// An ask for this user is out.
    pub fn is_loading(&self, user_id: &i64) -> bool {
        self.by_user.get(user_id).is_some_and(Fetch::is_loading)
    }

    /// Whether the last ask failed (and no answer has come since).
    pub fn failed(&self, user_id: &i64) -> bool {
        matches!(self.by_user.get(user_id), Some(Fetch::Failed { .. }))
    }

    /// An ask went out as `request`: the entry waits for it, keeping any
    /// answer held, and a failure of it lands as `Failed` (never a
    /// spinner nothing ends).
    pub fn begin(&mut self, user_id: i64, request: crate::ids::RequestId) {
        let entry = self.by_user.entry(user_id).or_default();
        entry.begin();
        entry.sent(request);
    }

    /// An ask is out or due and nothing was answered: a spinner shows. An
    /// entry that was never asked, or that failed, does not load.
    pub fn shows_loading(&self, user_id: &i64) -> bool {
        self.by_user
            .get(user_id)
            .is_some_and(|entry| entry.is_idle() || entry.shows_loading())
    }

    /// Why the last ask failed, until an answer or a retry.
    pub fn error(&self, user_id: &i64) -> Option<&str> {
        self.by_user.get(user_id)?.error()
    }

    /// An explicit retry: the failure is forgotten, the data kept.
    pub fn forget_failure(&mut self, user_id: &i64) {
        let Some(entry) = self.by_user.get_mut(user_id) else {
            return;
        };
        if matches!(entry, Fetch::Failed { .. }) {
            entry.clear_error();
            if entry.is_idle() {
                self.by_user.remove(user_id);
            }
        }
    }
}
