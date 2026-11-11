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

    /// An answer: the data, and no failure.
    pub fn insert(&mut self, user_id: i64, data: UserFullInfoData) {
        self.by_user.insert(user_id, Fetch::Ready(data));
    }

    pub fn remove(&mut self, user_id: &i64) {
        self.by_user.remove(user_id);
    }

    /// The ask for this user failed.
    pub fn fail(&mut self, user_id: i64, error: String) {
        self.by_user.entry(user_id).or_default().fail(error);
    }

    /// Whether the last ask failed (and no answer has come since).
    pub fn failed(&self, user_id: &i64) -> bool {
        matches!(self.by_user.get(user_id), Some(Fetch::Failed { .. }))
    }

    /// An explicit retry: the failure is forgotten, the data kept.
    pub fn forget_failure(&mut self, user_id: &i64) {
        let Some(entry) = self.by_user.get_mut(user_id) else {
            return;
        };
        entry.clear_error();
        if entry.is_idle() {
            self.by_user.remove(user_id);
        }
    }
}
