//! Connect driver: async and zoomed graphs, message statistics with
//! public forwards, and recent-post previews for the statistics panel.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{GraphKey, GroupsPurpose, MessageStatsFetch, RequestPurpose};
use crate::telegram::requests::get_message;
use crate::telegram::requests_statistics::{
    get_message_public_forwards, get_message_statistics, get_statistical_graph,
};

/// Public forwards are fetched in pages of this size (TDLib may return
/// fewer).
const FORWARDS_PAGE: i32 = 30;

impl<S: JsonSender> ConnectDriver<S> {
    /// `getStatisticalGraph` for an async graph (`x` 0) or one day of a
    /// zoomable graph. Skipped when the same graph is loading or loaded;
    /// a failed one is retried.
    pub fn load_stat_graph(
        &mut self,
        chat_id: ChatId,
        token: &str,
        x: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || token.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let stats = &self.session.groups.stats;
        if matches!(
            stats.graph(chat_id.0, token, x),
            Some(crate::state::GraphFetch::Loading | crate::state::GraphFetch::Loaded(_))
        ) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetStatisticalGraph, Some(chat_id));
        self.session.groups.stats.graph_requested(
            extra,
            GraphKey {
                chat_id: chat_id.0,
                token: token.to_string(),
                x,
            },
        );
        match self
            .sender
            .send_json(&get_statistical_graph(extra, chat_id, token, x))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.groups.stats.graph_unsent(extra);
                Err(err)
            }
        }
    }

    /// Zoom a graph into the day at `x`: remember the zoom and load that
    /// day (tdesktop `requestZoom`).
    pub fn zoom_stat_graph(
        &mut self,
        chat_id: ChatId,
        token: &str,
        x: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session
            .groups
            .stats
            .zoomed
            .insert(token.to_string(), x);
        self.load_stat_graph(chat_id, token, x)
    }

    /// "Zoom Out".
    pub fn unzoom_stat_graph(&mut self, token: &str) {
        self.session.groups.stats.zoomed.remove(token);
    }

    /// Open a post's statistics: `getMessageStatistics` plus the first
    /// `getMessagePublicForwards` page. The schema allows both only when
    /// `messageProperties.can_get_statistics`; the caller checks it.
    pub fn fetch_message_statistics(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Groups(GroupsPurpose::GetMessageStatistics {
            chat_id: chat_id.0,
            message_id: message_id.0,
        });
        self.session
            .open_message_stats_page(chat_id.0, message_id.0);
        let loaded = self
            .session
            .groups
            .stats
            .message_for(chat_id.0, message_id.0)
            .is_some_and(|state| matches!(state.statistics, MessageStatsFetch::Loaded(_)));
        if loaded || self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        if let Some(state) = self.session.groups.stats.message.as_mut() {
            state.statistics = MessageStatsFetch::Loading;
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &get_message_statistics(extra, chat_id, message_id, is_dark),
        )?;
        let forwards_loaded = self
            .session
            .groups
            .stats
            .message_for(chat_id.0, message_id.0)
            .is_some_and(|state| state.forwards.total > 0 || !state.forwards.forwards.is_empty());
        if !forwards_loaded {
            let _ = self.fetch_message_public_forwards(chat_id, message_id, "");
        }
        Ok(Some(extra))
    }

    /// One `getMessagePublicForwards` page; `offset` is the previous
    /// page's `next_offset` (empty for the first).
    pub fn fetch_message_public_forwards(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        offset: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Groups(GroupsPurpose::GetMessagePublicForwards {
            chat_id: chat_id.0,
            message_id: message_id.0,
        });
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &get_message_public_forwards(extra, chat_id, message_id, offset, FORWARDS_PAGE),
        )?;
        self.session
            .groups
            .stats
            .forwards_requested(chat_id.0, message_id.0);
        Ok(Some(extra))
    }

    /// The next page of the open post's public forwards.
    pub fn load_more_message_forwards(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(state) = self.session.groups.stats.message.as_ref() else {
            return Ok(None);
        };
        if state.forwards.loading || state.forwards.next_offset.is_empty() {
            return Ok(None);
        }
        let (chat_id, message_id) = (ChatId(state.chat_id), MessageId(state.message_id));
        let offset = state.forwards.next_offset.clone();
        self.fetch_message_public_forwards(chat_id, message_id, &offset)
    }

    /// Re-run a failed message statistics load.
    pub fn retry_message_statistics(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if self
            .session
            .groups
            .stats
            .message_for(chat_id.0, message_id.0)
            .is_some_and(|state| matches!(state.statistics, MessageStatsFetch::Failed(_)))
        {
            // Start over: statistics and the forwards list.
            self.session.groups.stats.message = None;
        }
        self.fetch_message_statistics(chat_id, message_id, is_dark)
    }

    /// `getMessage` for a recent post's row when the post is not in the
    /// loaded history (tdesktop `requestMessageData`). Once per post.
    pub fn fetch_stats_post(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Groups(GroupsPurpose::GetStatsPost {
            chat_id: chat_id.0,
            message_id: message_id.0,
        });
        if self
            .session
            .groups
            .stats
            .posts
            .contains_key(&(chat_id.0, message_id.0))
            || self.session.requests.has_purpose(purpose)
        {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(extra, &get_message(extra, chat_id, message_id))
            .map(Some)
    }

    /// `getPollVoteStatistics` for a poll's message statistics (tdesktop
    /// `fillPollVotesGraph`). Once per post; a refusal leaves the chart
    /// out.
    pub fn fetch_stats_poll_votes(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let key = (chat_id.0, message_id.0);
        if self.session.messages.poll_stats.contains_key(&key) {
            return Ok(None);
        }
        self.session
            .messages
            .poll_stats
            .insert(key, crate::state::PollStatsFetch::Loading);
        let purpose =
            RequestPurpose::Messages(crate::state::MessagesPurpose::GetPollVoteStatistics {
                chat_id,
                message_id,
            });
        let extra = self.session.request(purpose, Some(chat_id));
        let sent = self.send_json_request(
            extra,
            &crate::telegram::requests::get_poll_vote_statistics(
                extra, chat_id, message_id, is_dark,
            ),
        );
        if sent.is_err() {
            self.session.messages.poll_stats.remove(&key);
        }
        sent.map(Some)
    }
}
