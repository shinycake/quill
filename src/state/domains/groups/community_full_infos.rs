//! The `communityFullInfo` pack of every community, one `Fetch` per
//! community: the fetch's stage, its request and its answer live in the
//! one value, so "fetched", "in flight" and "failed" cannot disagree.
use std::collections::HashMap;

use crate::ids::RequestId;
use crate::state::fetch::Fetch;
use crate::telegram::envelope::ParsedCommunityFullInfo;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommunityFullInfos(HashMap<i64, Fetch<ParsedCommunityFullInfo>>);

impl CommunityFullInfos {
    /// The newest pack held for `community_id`.
    pub fn get(&self, community_id: &i64) -> Option<&ParsedCommunityFullInfo> {
        self.0.get(community_id).and_then(Fetch::data)
    }

    pub fn get_mut(&mut self, community_id: &i64) -> Option<&mut ParsedCommunityFullInfo> {
        self.0.get_mut(community_id).and_then(Fetch::data_mut)
    }

    /// Whether a pack is held (a refetch in flight keeps the old one).
    pub fn contains_key(&self, community_id: &i64) -> bool {
        self.get(community_id).is_some()
    }

    /// Every pack held, with its community id.
    pub fn iter(&self) -> impl Iterator<Item = (&i64, &ParsedCommunityFullInfo)> {
        self.0
            .iter()
            .filter_map(|(id, fetch)| fetch.data().map(|info| (id, info)))
    }

    pub fn values(&self) -> impl Iterator<Item = &ParsedCommunityFullInfo> {
        self.iter().map(|(_, info)| info)
    }

    /// Where the fetch for `community_id` stands.
    pub fn fetch(&self, community_id: i64) -> Option<&Fetch<ParsedCommunityFullInfo>> {
        self.0.get(&community_id)
    }

    /// TDLib pushed the pack (`updateCommunityFullInfo`): it replaces the
    /// held one, and a request in flight is no longer waited for.
    pub fn insert(&mut self, community_id: i64, info: ParsedCommunityFullInfo) {
        self.0.insert(community_id, Fetch::Ready(info));
    }

    /// The community is gone (deleted, left): its pack and fetch go.
    pub fn remove(&mut self, community_id: &i64) {
        self.0.remove(community_id);
    }

    /// The pack is out of date (a change landed): the old one stays until
    /// the refetch replaces it, and an ask in flight is dropped.
    pub fn outdate(&mut self, community_id: i64) {
        if let Some(fetch) = self.0.get_mut(&community_id) {
            fetch.outdate();
        }
    }

    /// A refetch is owed and nothing is in flight: never asked, or out of
    /// date. The background asks (a collapsed row, the moderation ban) go
    /// out on this, so a failure waits for a change.
    pub fn wants_fetch(&self, community_id: i64) -> bool {
        self.0.get(&community_id).is_none_or(Fetch::wants_fetch)
    }

    /// A person's own ask (opening the info panel, entering the mode): it
    /// also retries after a failure.
    pub fn may_ask(&self, community_id: i64) -> bool {
        self.0
            .get(&community_id)
            .is_none_or(|fetch| fetch.wants_fetch() || matches!(fetch, Fetch::Failed { .. }))
    }

    /// The ask for `community_id` is about to go out.
    pub fn begin(&mut self, community_id: i64) {
        self.0.entry(community_id).or_default().begin();
    }

    /// The ask went out as `request`.
    pub fn sent(&mut self, community_id: i64, request: RequestId) {
        if let Some(fetch) = self.0.get_mut(&community_id) {
            fetch.sent(request);
        }
    }

    /// The send was refused (the request never left): back to before it.
    pub fn cancel(&mut self, community_id: i64) {
        if let Some(fetch) = self.0.get_mut(&community_id) {
            fetch.cancel();
        }
    }

    /// `request` answered with the pack; only the ask that waits for it
    /// takes it.
    pub fn answered(
        &mut self,
        community_id: i64,
        request: Option<RequestId>,
        info: ParsedCommunityFullInfo,
    ) -> bool {
        self.0
            .get_mut(&community_id)
            .is_some_and(|fetch| fetch.answered(request, info))
    }

    /// `request` failed: the background asks wait for a change.
    pub fn failed(&mut self, community_id: i64, request: Option<RequestId>, error: &str) -> bool {
        self.0
            .get_mut(&community_id)
            .is_some_and(|fetch| fetch.failed(request, error, false))
    }

    /// TDLib sent something new about the community (`updateCommunity`):
    /// a failed fetch may be asked again.
    pub fn forget_failure(&mut self, community_id: i64) {
        if let Some(fetch) = self.0.get_mut(&community_id) {
            fetch.clear_error();
        }
    }
}

#[cfg(test)]
impl std::ops::Index<&i64> for CommunityFullInfos {
    type Output = ParsedCommunityFullInfo;

    fn index(&self, community_id: &i64) -> &Self::Output {
        self.get(community_id).expect("no community full info")
    }
}
