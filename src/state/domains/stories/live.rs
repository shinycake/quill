//! The live story open in the story viewer: the streamer, top donors, who
//! comments are sent as, the pending Star reaction and the comments
//! toggle. The call itself is the story stream (`calls.story_stream`);
//! its comments are that call's messages.
use crate::live_story::{CommentsTracker, PendingLiveReaction, normalize_levels, retain_comments};
use crate::state::*;
use crate::telegram::envelope::{
    AvailableMessageSender, GroupCallMessageLevel, LiveStoryDonors, LiveStoryPayload,
    ParsedGroupCallMessage, ParsedGroupCallParticipant,
};

/// In-flight live story requests; wrapped as [`StoriesPurpose::Live`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveStoryPurpose {
    /// `getLiveStoryStreamer`; answered with `groupCallParticipant`.
    GetStreamer { group_call_id: i32 },
    /// `getLiveStoryTopDonors`; answered with `liveStoryDonors`.
    GetTopDonors { group_call_id: i32 },
    /// `getLiveStoryAvailableMessageSenders`; answered with
    /// `chatMessageSenders`.
    GetMessageSenders { group_call_id: i32 },
    /// `setLiveStoryMessageSender`; the choice arrives as `updateGroupCall`.
    SetMessageSender { group_call_id: i32 },
    /// `sendGroupCallMessage`; the comment arrives as
    /// `updateNewGroupCallMessage`.
    SendComment { group_call_id: i32 },
    /// `addPendingLiveStoryReaction`; a failure takes the Stars back.
    AddPendingReaction { group_call_id: i32, stars: i64 },
    /// `commitPendingLiveStoryReactions`.
    CommitPendingReactions { group_call_id: i32 },
    /// `removePendingLiveStoryReactions` (Undo, or leaving the story).
    RemovePendingReactions { group_call_id: i32 },
    /// `deleteGroupCallMessages` / `deleteGroupCallMessagesBySender`.
    DeleteMessages { group_call_id: i32 },
}

/// Stars someone just added: the viewer flies a badge up from the Stars
/// button (tdesktop `StarEffect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveStarEffect {
    pub sender: MessageSender,
    pub stars: i64,
}

/// One live story being watched.
#[derive(Debug, Clone)]
pub struct LiveStoryState {
    pub chat_id: i64,
    pub story_id: i32,
    pub group_call_id: i32,
    /// The streamer, top donors and comment senders were asked for.
    pub extras_requested: bool,
    /// `getLiveStoryStreamer` (not for RTMP streams): whose video to play.
    pub streamer: Option<ParsedGroupCallParticipant>,
    pub donors: Option<LiveStoryDonors>,
    /// Who comments may be sent as; `None` until loaded.
    pub message_senders: Option<Vec<AvailableMessageSender>>,
    /// Stars tapped and not committed yet.
    pub pending_reaction: Option<PendingLiveReaction>,
    /// Stars committed and not yet in a donors update: the total on the
    /// Stars button keeps them until TDLib counts them.
    pub committed_stars: i64,
    /// New Stars to show as effects, taken by the viewer.
    pub effects: Vec<LiveStarEffect>,
    /// Your comments TDLib could not send.
    pub failed: HashSet<i32>,
    pub comments: CommentsTracker,
    /// The last refusal or failure, shown in the viewer.
    pub note: Option<String>,
}

impl LiveStoryState {
    pub fn new(chat_id: i64, story_id: i32, group_call_id: i32) -> Self {
        Self {
            chat_id,
            story_id,
            group_call_id,
            extras_requested: false,
            streamer: None,
            donors: None,
            message_senders: None,
            pending_reaction: None,
            committed_stars: 0,
            effects: Vec::new(),
            failed: HashSet::new(),
            comments: CommentsTracker::default(),
            note: None,
        }
    }

    /// Stars on the Stars button: everyone's total plus your pending and
    /// just committed ones (tdesktop `starsLocalState().total`).
    pub fn stars_total(&self) -> i64 {
        self.donors
            .as_ref()
            .map_or(0, |donors| donors.total_star_count)
            .saturating_add(self.unconfirmed_stars())
    }

    /// Your Stars the donors list does not count yet.
    fn unconfirmed_stars(&self) -> i64 {
        self.pending_reaction
            .map_or(0, |p| p.stars)
            .saturating_add(self.committed_stars)
    }

    /// The pending Stars went out.
    pub(crate) fn commit_reaction(&mut self) -> Option<PendingLiveReaction> {
        let pending = self.pending_reaction.take()?;
        self.committed_stars = self.committed_stars.saturating_add(pending.stars);
        Some(pending)
    }

    /// New donors from TDLib: they count everything committed so far.
    pub(crate) fn set_donors(&mut self, donors: LiveStoryDonors) {
        self.donors = Some(donors);
        self.committed_stars = 0;
    }

    /// Queues a Stars effect, keeping the newest four.
    pub(crate) fn push_effect(&mut self, sender: MessageSender, stars: i64) {
        if stars <= 0 {
            return;
        }
        self.effects.push(LiveStarEffect { sender, stars });
        let over = self.effects.len().saturating_sub(MAX_QUEUED_EFFECTS);
        self.effects.drain(..over);
    }

    /// Your Stars: committed plus pending.
    pub fn my_stars(&self) -> i64 {
        self.donors
            .as_ref()
            .and_then(|donors| donors.top_donors.iter().find(|d| d.is_me))
            .map_or(0, |mine| mine.star_count)
            .saturating_add(self.unconfirmed_stars())
    }
}

/// tdesktop keeps at most four effects of each kind at once.
const MAX_QUEUED_EFFECTS: usize = 4;

impl Session {
    /// Starts watching a live story (the join is on its way).
    pub fn begin_live_story(&mut self, chat_id: i64, story_id: i32, group_call_id: i32) {
        if self
            .stories
            .live
            .as_ref()
            .is_some_and(|live| live.group_call_id == group_call_id)
        {
            return;
        }
        self.stories.live = Some(LiveStoryState::new(chat_id, story_id, group_call_id));
    }

    /// Stops watching (the viewer left the story).
    pub fn end_live_story(&mut self) {
        self.stories.live = None;
        self.calls.story_stream = None;
    }

    /// The live story behind `group_call_id`, when it is the one watched.
    pub fn live_story_mut(&mut self, group_call_id: i32) -> Option<&mut LiveStoryState> {
        self.stories
            .live
            .as_mut()
            .filter(|live| live.group_call_id == group_call_id)
    }

    /// The paid comment levels, lowest first.
    pub fn live_levels(&self) -> Vec<GroupCallMessageLevel> {
        if self.stories.live_levels.is_empty() {
            crate::live_story::fallback_levels()
        } else {
            self.stories.live_levels.clone()
        }
    }

    /// The most Stars one comment or one batch of reactions may carry
    /// (tdesktop `MaxVideoStreamStarsCount`).
    pub fn live_max_stars(&self) -> i64 {
        self.stories
            .live_max_stars
            .unwrap_or(crate::live_story::FALLBACK_MAX_STARS)
            .max(2)
    }

    /// Stars you can still spend on the watched live story: the balance
    /// less the Stars waiting on messages and on this story's reaction
    /// (tdesktop counts both against `starsLocalState`). `None` while the
    /// balance is unknown.
    pub fn live_spendable_stars(&self) -> Option<i64> {
        let balance = self.payments.hub.balance.map(|b| b.stars)?;
        let reaction = self
            .stories
            .live
            .as_ref()
            .and_then(|live| live.pending_reaction)
            .map_or(0, |p| p.stars);
        Some(balance - self.payments.paid.total_pending() - reaction)
    }

    /// Whether `stars` more would go past what you can spend (tdesktop
    /// `MaybeRequestBalanceIncrease` then asks for a top-up).
    pub fn live_needs_top_up(&self, stars: i64) -> bool {
        stars > 0 && self.live_spendable_stars().is_some_and(|have| have < stars)
    }

    /// Who your live story comments are sent as: the call's choice, else
    /// yourself.
    pub fn live_story_sender(&self) -> Option<MessageSender> {
        self.calls
            .story_stream
            .as_ref()
            .and_then(|call| call.message_sender_id)
            .or_else(|| {
                self.my_user_id
                    .map(|user_id| MessageSender::User { user_id })
            })
    }

    pub(crate) fn apply_live_story_payload(
        &mut self,
        payload: LiveStoryPayload,
        pending: Option<&PendingRequest>,
    ) {
        let asked = match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Stories(StoriesPurpose::Live(purpose))) => Some(purpose),
            _ => None,
        };
        match payload {
            LiveStoryPayload::UpdateTopDonors {
                group_call_id,
                donors,
            } => {
                if let Some(live) = self.live_story_mut(group_call_id) {
                    live.set_donors(donors);
                }
            }
            LiveStoryPayload::Donors(donors) => {
                if let Some(LiveStoryPurpose::GetTopDonors { group_call_id }) = asked
                    && let Some(live) = self.live_story_mut(group_call_id)
                {
                    live.set_donors(donors);
                }
            }
            LiveStoryPayload::UpdateMessageLevels(levels) => {
                self.stories.live_levels = normalize_levels(levels);
            }
            LiveStoryPayload::UpdateNewPaidReaction {
                group_call_id,
                sender_id,
                star_count,
            } => {
                // Your own reaction got its effect when it was committed.
                let me = self.live_story_sender();
                let mine = Some(sender_id) == me
                    || matches!(sender_id, MessageSender::User { user_id } if Some(user_id) == self.my_user_id);
                if let Some(live) = self.live_story_mut(group_call_id)
                    && !mine
                {
                    push_effect(live, sender_id, star_count);
                }
            }
            LiveStoryPayload::Streamer(participant) => {
                if let Some(LiveStoryPurpose::GetStreamer { group_call_id }) = asked
                    && let Some(live) = self.live_story_mut(group_call_id)
                {
                    live.streamer = Some(participant);
                }
            }
        }
    }

    /// The `chatMessageSenders` answer to `getLiveStoryAvailableMessageSenders`.
    pub(crate) fn set_live_story_senders(
        &mut self,
        group_call_id: i32,
        senders: Vec<AvailableMessageSender>,
    ) {
        if let Some(live) = self.live_story_mut(group_call_id) {
            live.message_senders = Some(senders);
        }
    }

    /// A live story comment arrived (already appended to the call's
    /// messages): drop a paid reaction below the comment price, trim old
    /// comments, queue the Stars effect and update the toggle.
    pub(crate) fn after_live_story_message(
        &mut self,
        group_call_id: i32,
        message: &ParsedGroupCallMessage,
    ) {
        let levels = self.live_levels();
        let me = self.live_story_sender();
        let my_user = self.my_user_id;
        let now = i64::try_from(unix_ms_now() / 1000).unwrap_or(i64::MAX);
        let Some(call) = self
            .calls
            .story_stream
            .as_mut()
            .filter(|call| call.id == group_call_id)
        else {
            return;
        };
        // tdesktop `skipMessage`: a paid reaction under the comment price
        // only counts toward the donors.
        if message.text.is_empty() && message.paid_message_star_count < call.paid_message_star_count
        {
            call.messages.retain(|m| m.message_id != message.message_id);
        }
        call.messages.sort_by_key(|m| m.message_id);
        retain_comments(&mut call.messages, &levels, now);
        let messages = call.messages.clone();
        let mine = Some(message.sender_id) == me
            || matches!(message.sender_id, MessageSender::User { user_id } if Some(user_id) == my_user);
        if let Some(live) = self.live_story_mut(group_call_id) {
            if message.paid_message_star_count > 0 && !mine {
                push_effect(live, message.sender_id, message.paid_message_star_count);
            }
            live.comments.on_messages(&messages, me);
        }
    }

    /// Comments were deleted: the toggle follows.
    pub(crate) fn after_live_story_messages_deleted(&mut self, group_call_id: i32) {
        let me = self.live_story_sender();
        let messages = match self.calls.story_stream.as_ref() {
            Some(call) if call.id == group_call_id => call.messages.clone(),
            _ => return,
        };
        if let Some(live) = self.live_story_mut(group_call_id) {
            live.comments.on_messages(&messages, me);
        }
    }

    /// `updateGroupCallMessageSendFailed` for a live story comment.
    pub(crate) fn live_story_message_failed(
        &mut self,
        group_call_id: i32,
        message_id: i32,
    ) -> bool {
        match self.live_story_mut(group_call_id) {
            Some(live) => {
                live.failed.insert(message_id);
                true
            }
            None => false,
        }
    }

    /// A live story request failed.
    pub(crate) fn apply_live_story_error(&mut self, purpose: LiveStoryPurpose, err: &TdError) {
        let group_call_id = match purpose {
            LiveStoryPurpose::GetStreamer { group_call_id }
            | LiveStoryPurpose::GetTopDonors { group_call_id }
            | LiveStoryPurpose::GetMessageSenders { group_call_id }
            | LiveStoryPurpose::SetMessageSender { group_call_id }
            | LiveStoryPurpose::SendComment { group_call_id }
            | LiveStoryPurpose::AddPendingReaction { group_call_id, .. }
            | LiveStoryPurpose::CommitPendingReactions { group_call_id }
            | LiveStoryPurpose::RemovePendingReactions { group_call_id }
            | LiveStoryPurpose::DeleteMessages { group_call_id } => group_call_id,
        };
        let Some(live) = self.live_story_mut(group_call_id) else {
            return;
        };
        let reason = error_reason(err);
        match purpose {
            // Nothing to show: the viewer keeps its defaults.
            LiveStoryPurpose::GetStreamer { .. }
            | LiveStoryPurpose::GetTopDonors { .. }
            | LiveStoryPurpose::GetMessageSenders { .. } => {}
            LiveStoryPurpose::AddPendingReaction { stars, .. } => {
                if let Some(pending) = live.pending_reaction.as_mut() {
                    pending.stars = pending.stars.saturating_sub(stars);
                    if pending.stars <= 0 {
                        live.pending_reaction = None;
                    }
                }
                live.note = Some(format!("Stars were not sent: {reason}"));
            }
            LiveStoryPurpose::CommitPendingReactions { .. } => {
                live.committed_stars = 0;
                live.note = Some(format!("Stars were not sent: {reason}"));
            }
            LiveStoryPurpose::SendComment { .. } => {
                live.note = Some(format!("Could not send the comment: {reason}"));
            }
            LiveStoryPurpose::SetMessageSender { .. } => {
                live.note = Some(format!("Could not change who you comment as: {reason}"));
            }
            LiveStoryPurpose::RemovePendingReactions { .. }
            | LiveStoryPurpose::DeleteMessages { .. } => {
                live.note = Some(reason.to_string());
            }
        }
    }
}

fn push_effect(live: &mut LiveStoryState, sender: MessageSender, stars: i64) {
    live.push_effect(sender, stars);
}

#[cfg(test)]
mod tests;
