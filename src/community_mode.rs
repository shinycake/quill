//! Parity slice `parity:communities-chatlist-mode`: community chat-list
//! mode — view a community's chats as a filtered chat list.
//!
//! Membership is derived from the cached `communityFullInfo.chats` pack
//! (schema 1.8.67, line 2319): state keeps no per-chat `community_id`,
//! so the pack is the source of truth. `updateCommunityFullInfo`
//! (line 10753) replaces the pack wholesale, so a chat leaving the
//! community drops out of the filtered list on the next render with no
//! extra sync.

use std::collections::{HashMap, HashSet};

use crate::state::ChatSummary;
use crate::telegram::envelope::ParsedCommunityFullInfo;

/// The member chat ids of a community from its cached full-info pack.
/// `None` (never fetched) yields an empty set — the chat-list renderer
/// shows a loading state for that case.
///
/// Hidden chats (`communityChat.is_hidden`, "visible only to community
/// administrators") are included: the mode only ever *narrows* the
/// already-visible main chat list, so it cannot surface anything the
/// user couldn't already see, and the hub lists owned communities.
pub fn community_member_ids(full_info: Option<&ParsedCommunityFullInfo>) -> HashSet<i64> {
    full_info
        .map(|info| info.chats.iter().map(|chat| chat.chat_id).collect())
        .unwrap_or_default()
}

/// Narrow `chats` to `community_id`'s member chats in place. `None`
/// (mode cleared / never entered) keeps every chat; a missing full-info
/// pack keeps none (the caller shows "Loading…" for that case).
///
/// This is the single production predicate for the community chat-list
/// mode — the UI calls it directly, and the tests exercise it through
/// the real `Session` reducer path, so there is exactly one
/// implementation to keep in sync.
pub fn retain_community_chats<C: std::borrow::Borrow<ChatSummary>>(
    chats: &mut Vec<C>,
    community_id: Option<i64>,
    full_infos: &HashMap<i64, ParsedCommunityFullInfo>,
) {
    let Some(community_id) = community_id else {
        return;
    };
    let members = community_member_ids(full_infos.get(&community_id));
    chats.retain(|chat| members.contains(&chat.borrow().id.0));
}
