//! Finished community requests: the toast tdesktop shows, and the box
//! that asked closes.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::lang::trn;
use quill::state::{CommunityError, CommunityOutcome, DEFAULT_COMMUNITY_PEERS_MAX};

impl QuillApp {
    /// The limit toast (`CommunityPeersLimitToast`), for the chat being
    /// added.
    fn community_limit_text(&self, chat_id: Option<i64>) -> String {
        let bot = chat_id.is_some_and(|id| self.community_chat_kind(id) == CommunityChatKind::Bot);
        let limits = self.session().map(|s| &s.communities);
        let (key, count) = if bot {
            (
                keys::COMMUNITY_BOT_PEERS_LIMIT,
                limits.map_or(DEFAULT_COMMUNITY_PEERS_MAX, |l| l.bot_count_max),
            )
        } else {
            (
                keys::COMMUNITY_PEERS_LIMIT,
                limits.map_or(DEFAULT_COMMUNITY_PEERS_MAX, |l| l.chat_count_max),
            )
        };
        trn(key, count)
    }

    fn community_error_text(&self, error: &CommunityError, chat_id: Option<i64>) -> String {
        match error {
            CommunityError::PeersTooMuch => self.community_limit_text(chat_id),
            CommunityError::Other(reason) => reason.clone(),
        }
    }

    /// Whether the open box is the one that asked to add `chat_id`.
    fn box_adds(&self, community_id: i64, chat_id: i64) -> bool {
        matches!(
            self.admin.community.flow.open,
            Some(CommunityBox::Visibility { community_id: c, chat_id: h, .. }
                | CommunityBox::ConfirmAdd { community_id: c, chat_id: h, .. })
                if c == community_id && h == chat_id
        )
    }

    fn apply_community_outcome(&mut self, outcome: CommunityOutcome, cx: &mut Context<Self>) {
        let note = match outcome {
            CommunityOutcome::ChatAdded {
                community_id,
                chat_id,
            } => {
                if self.box_adds(community_id, chat_id) {
                    self.admin.community.flow.open = None;
                }
                StatusNote::info(by_kind(
                    self.community_chat_kind(chat_id),
                    keys::COMMUNITY_ADD_DONE,
                    keys::COMMUNITY_ADD_DONE_CHANNEL,
                    keys::COMMUNITY_ADD_DONE_BOT,
                ))
            }
            CommunityOutcome::AddRequestSent {
                community_id,
                chat_id,
            } => {
                if self.box_adds(community_id, chat_id) {
                    self.admin.community.flow.open = None;
                }
                StatusNote::info(tr(keys::COMMUNITY_REQUEST_SENT))
            }
            CommunityOutcome::AddFailed { chat_id, error, .. } => {
                StatusNote::error(self.community_error_text(&error, Some(chat_id)))
            }
            CommunityOutcome::ChatRemoved { .. } => {
                StatusNote::info(tr(keys::COMMUNITY_REMOVE_DONE))
            }
            CommunityOutcome::RemoveFailed { reason, .. }
            | CommunityOutcome::BanFailed { reason, .. }
            | CommunityOutcome::CollapseFailed { reason, .. }
            | CommunityOutcome::NotificationsFailed { reason, .. } => StatusNote::error(reason),
            CommunityOutcome::RequestProcessed { .. } => return,
            CommunityOutcome::AllRequestsProcessed {
                community_id,
                approve,
                count,
            } => {
                if matches!(
                    self.admin.community.flow.open,
                    Some(CommunityBox::ConfirmProcessAll { community_id: c, .. }) if c == community_id
                ) {
                    self.admin.community.flow.open = None;
                }
                StatusNote::info(trn(
                    if approve {
                        keys::COMMUNITY_REQUEST_ADDED_TOAST
                    } else {
                        keys::COMMUNITY_REQUEST_DECLINED_TOAST
                    },
                    count,
                ))
            }
            CommunityOutcome::ProcessFailed { error, .. } => {
                StatusNote::error(self.community_error_text(&error, None))
            }
            CommunityOutcome::Created { chat_id, .. } => {
                if let Some(CommunityBox::CreateVisibility { chat_id: asked, .. }) =
                    &self.admin.community.flow.open
                    && Some(*asked) == chat_id
                {
                    self.admin.community.flow.open = None;
                }
                StatusNote::info(tr(keys::COMMUNITY_CREATED))
            }
            CommunityOutcome::CreateFailed { reason } => StatusNote::error(reason),
            CommunityOutcome::MemberBanned { member, .. } => StatusNote::info(tr_with(
                keys::COMMUNITY_BAN_DONE,
                &[("user", &self.community_member_name(member))],
            )),
        };
        self.connection.set_note(note);
        cx.notify();
    }

    /// Poll drain: report finished community requests, move a waiting
    /// ban on, and fetch what collapsed rows need.
    pub(in crate::ui) fn drain_communities(&mut self, cx: &mut Context<Self>) -> bool {
        let mut progressed = false;
        while let Some(outcome) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.communities.take_outcome())
        {
            self.apply_community_outcome(outcome, cx);
            progressed = true;
        }
        progressed |= self.drain_pending_ban(cx);
        progressed |= self.drain_new_community_chat(cx);
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.sync_collapsed_communities()
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
        }
        progressed
    }
}
