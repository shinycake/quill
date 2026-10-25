//! The similar-channels strip under "You joined this channel" (tdesktop
//! `HistoryView::SimilarChannels`): what the reducer keeps for it, the tile
//! model, and the synthetic service rows the history list shows for it and
//! for a thread's root divider.
use super::*;
use crate::telegram::envelope::ServiceAction;

/// Row id of the "You joined this channel" notice: never a TDLib id
/// (TDLib ids are positive; Quill's pending sends use small negatives).
pub const JOINED_NOTICE_ROW_ID: i64 = -9_000_000_001;
/// Row id of the divider between a thread's root post and its replies.
pub const THREAD_DIVIDER_ROW_ID: i64 = -9_000_000_002;

/// A channel joined in this session: the joined notice sits at the end of
/// its history with the strip under it (tdesktop `GenerateJoinedMessage`
/// plus the channel's `SimilarExpanded` flag, which `joinChannel` sets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimilarStrip {
    /// Unix time of the join: the notice's date.
    pub joined_at: i32,
    /// Clicking the notice collapses or expands the strip.
    pub expanded: bool,
}

/// One tile of the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimilarTile {
    /// A similar channel: its avatar, subscriber badge and name.
    Channel { chat_id: i64 },
    /// "More Channels" (tdesktop `lng_similar_channels_more`): replaces the
    /// last listed tile when more channels exist than the strip shows.
    /// `stack` holds up to three channels for the stacked avatars (the
    /// replaced one first); `locked` marks the Premium gate.
    More {
        count: i32,
        locked: bool,
        stack: Vec<i64>,
    },
}

/// tdesktop `SimilarChannels::countOptimalSize`: the first `limit` channels
/// are shown; when the server counted more than the list holds (or the
/// list is more than twice the limit), the last shown tile becomes "More
/// Channels" counting everything beyond it. Without Premium that tile is
/// locked.
pub fn similar_tiles(
    list: &[i64],
    total_count: i32,
    premium: bool,
    limit: usize,
) -> Vec<SimilarTile> {
    let beyond_list = total_count.saturating_sub(list.len() as i32).max(0);
    let take = if beyond_list > 0 || list.len() > 2 * limit {
        limit.min(list.len())
    } else {
        list.len()
    };
    let more = beyond_list + (list.len() - take) as i32;
    list.iter()
        .take(take)
        .enumerate()
        .map(|(ix, &chat_id)| {
            if more > 0 && ix + 1 == take {
                SimilarTile::More {
                    count: more,
                    locked: !premium,
                    stack: list.iter().skip(ix).take(3).copied().collect(),
                }
            } else {
                SimilarTile::Channel { chat_id }
            }
        })
        .collect()
}

/// A Quill-local service row (never sent anywhere: its id is negative, so
/// `report_visible_messages` skips it).
pub(crate) fn synthetic_service_row(
    chat_id: ChatId,
    id: i64,
    date: i32,
    action: ServiceAction,
) -> HistoryMessage {
    HistoryMessage {
        sender: None,
        id: MessageId(id),
        chat_id,
        is_outgoing: false,
        date,
        content: MessageContent::Action(Box::new(action)),
        pending: false,
        reply_to: None,
        forward_info: None,
        extras: MessageExtras::default(),
        interaction_info: None,
        is_pinned: false,
        media_album_id: 0,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
        author_signature: None,
        failed: false,
        can_retry: false,
        ephemeral: None,
    }
}

impl Session {
    /// A `joinChat` succeeded: a channel gets its joined notice with the
    /// strip expanded (tdesktop `ApiWrap::joinChannel` sets
    /// `SimilarExpanded`); groups show nothing.
    pub(crate) fn note_channel_joined(&mut self, chat_id: ChatId) {
        if !self
            .chats
            .get(&chat_id.0)
            .is_some_and(ChatSummary::is_channel)
        {
            return;
        }
        self.groups.similar_strips.insert(
            chat_id.0,
            SimilarStrip {
                joined_at: crate::local_time::now_unix() as i32,
                expanded: true,
            },
        );
    }

    /// The strip state of a channel joined in this session.
    pub fn similar_strip(&self, chat_id: ChatId) -> Option<&SimilarStrip> {
        self.groups.similar_strips.get(&chat_id.0)
    }

    /// Clicking the joined notice collapses or expands the strip (tdesktop
    /// `SimilarChannels::ensureToggleLink`). Returns the new state.
    pub fn toggle_similar_strip(&mut self, chat_id: ChatId) -> Option<bool> {
        let strip = self.groups.similar_strips.get_mut(&chat_id.0)?;
        strip.expanded = !strip.expanded;
        // History rows are rebuilt on a revision change.
        self.revision = self.revision.wrapping_add(1);
        Some(strip.expanded)
    }

    /// The "You joined this channel" row at the end of a joined channel's
    /// history.
    pub fn joined_notice_row(&self, chat_id: ChatId) -> Option<HistoryMessage> {
        let strip = self.similar_strip(chat_id)?;
        Some(synthetic_service_row(
            chat_id,
            JOINED_NOTICE_ROW_ID,
            strip.joined_at,
            ServiceAction::YouJoinedChannel,
        ))
    }

    /// The strip's tiles for a joined channel: `None` until the
    /// `getChatSimilarChats` answer is in (or while collapsed); an empty
    /// list hides the strip, as tdesktop's `_empty`.
    pub fn similar_strip_tiles(&self, chat_id: ChatId) -> Option<Vec<SimilarTile>> {
        let strip = self.similar_strip(chat_id)?;
        if !strip.expanded {
            return None;
        }
        let ProfileChatsFetch::Loaded(list) = self
            .users_state
            .profile_chat_lists
            .get(&(ProfileChatsKind::SimilarChats, chat_id.0))?
        else {
            return None;
        };
        let total = self
            .users_state
            .similar_chat_totals
            .get(&chat_id.0)
            .copied()
            .unwrap_or(0);
        // tdesktop `PremiumLimits::similarChannelsDefault`: the free value
        // of `premiumLimitTypeSimilarChatCount`.
        let limit = self
            .premium_limit_pair(crate::premium_limits::PremiumLimitKind::SimilarChats)
            .default_value;
        Some(similar_tiles(
            list,
            total,
            self.is_premium(),
            usize::try_from(limit).unwrap_or(0),
        ))
    }

    /// `chats.total_count` of a `getChatSimilarChats` answer: how many
    /// similar channels exist, listed or not.
    pub(crate) fn note_similar_chat_total(
        &mut self,
        total_count: i32,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetProfileChats(ProfileChatsKind::SimilarChats)
            && let Some(chat_id) = pending.chat_id
        {
            self.users_state
                .similar_chat_totals
                .insert(chat_id.0, total_count);
        }
    }
}
