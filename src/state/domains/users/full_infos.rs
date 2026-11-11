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
        match self.by_user.get(user_id)? {
            Fetch::Ready(data) => Some(data),
            Fetch::Loading { stale } | Fetch::Failed { stale, .. } => stale.as_ref(),
            Fetch::Idle => None,
        }
    }

    pub fn get_mut(&mut self, user_id: &i64) -> Option<&mut UserFullInfoData> {
        match self.by_user.get_mut(user_id)? {
            Fetch::Ready(data) => Some(data),
            Fetch::Loading { stale } | Fetch::Failed { stale, .. } => stale.as_mut(),
            Fetch::Idle => None,
        }
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
        let stale = match self.by_user.remove(&user_id) {
            Some(Fetch::Ready(data)) => Some(data),
            Some(Fetch::Loading { stale } | Fetch::Failed { stale, .. }) => stale,
            Some(Fetch::Idle) | None => None,
        };
        self.by_user.insert(user_id, Fetch::Failed { error, stale });
    }

    /// Whether the last ask failed (and no answer has come since).
    pub fn failed(&self, user_id: &i64) -> bool {
        matches!(self.by_user.get(user_id), Some(Fetch::Failed { .. }))
    }

    /// An explicit retry: the failure is forgotten, the data kept.
    pub fn forget_failure(&mut self, user_id: &i64) {
        match self.by_user.remove(user_id) {
            Some(Fetch::Failed {
                stale: Some(data), ..
            }) => {
                self.by_user.insert(*user_id, Fetch::Ready(data));
            }
            Some(other @ (Fetch::Ready(_) | Fetch::Loading { .. })) => {
                self.by_user.insert(*user_id, other);
            }
            Some(Fetch::Failed { stale: None, .. } | Fetch::Idle) | None => {}
        }
    }
}
