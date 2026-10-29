//! Connect driver: the live story being watched. After the `joinLiveStory`
//! answer it asks for the streamer, the top donors and who comments may be
//! sent as, and once per session the Stars effect's bursts; it sends
//! comments and holds Star reactions for the Undo window (tdesktop
//! `Messages::reactionsPaidAdd`, `kPaidAccumulatePeriod`).
//! Nothing here spends Stars unless the viewer asked for it.
use super::*;
use crate::calls::live_video::LiveKey;
use crate::live_story::{PendingLiveReaction, comment_text};
use crate::paid_reaction::Refusal;
use crate::state::{LiveBursts, LiveStoryPurpose, StoriesPurpose};
use crate::telegram::requests::{
    add_pending_live_story_reaction, commit_pending_live_story_reactions,
    delete_group_call_messages, delete_group_call_messages_by_sender,
    get_custom_emoji_reaction_animations, get_live_story_available_message_senders,
    get_live_story_streamer, get_live_story_top_donors, remove_pending_live_story_reactions,
    send_live_story_comment, set_live_story_message_sender,
};

/// Why the story stream stopped, or never started: a call you are in for
/// the same chat holds the chat's stream.
const STORY_CHAT_TAKEN: &str = "a call in this chat is using its stream";

impl<S: JsonSender> ConnectDriver<S> {
    fn live_story_request(
        &mut self,
        purpose: LiveStoryPurpose,
        json: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::Stories(StoriesPurpose::Live(purpose)), None);
        let json = json(extra);
        self.send_json_request(extra, &json)
    }

    fn watched_live_call(&self) -> Option<&crate::state::ActiveGroupCall> {
        let live = self.session.stories.live.as_ref()?;
        self.session
            .calls
            .story_stream
            .as_ref()
            .filter(|call| call.id == live.group_call_id)
    }

    /// Once the `joinLiveStory` answer is in: `getLiveStoryStreamer` (not
    /// for RTMP streams), `getLiveStoryTopDonors` and
    /// `getLiveStoryAvailableMessageSenders`, together. Called from
    /// `ingest`.
    pub(crate) fn maybe_fetch_live_story_extras(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some((group_call_id, is_rtmp)) = self
            .watched_live_call()
            .filter(|call| call.is_joined || !call.join_payload.is_empty())
            .map(|call| (call.id, call.is_rtmp_stream))
        else {
            return Ok(());
        };
        let Some(live) = self.session.live_story_mut(group_call_id) else {
            return Ok(());
        };
        if live.extras_requested {
            return Ok(());
        }
        live.extras_requested = true;
        if !is_rtmp {
            self.live_story_request(LiveStoryPurpose::GetStreamer { group_call_id }, |extra| {
                get_live_story_streamer(extra, group_call_id)
            })?;
        }
        self.live_story_request(LiveStoryPurpose::GetTopDonors { group_call_id }, |extra| {
            get_live_story_top_donors(extra, group_call_id)
        })?;
        self.live_story_request(
            LiveStoryPurpose::GetMessageSenders { group_call_id },
            |extra| get_live_story_available_message_senders(extra, group_call_id),
        )?;
        self.maybe_fetch_live_bursts()
    }

    /// The Stars effect's bursts, once per session (tdesktop asks for the
    /// generic animations once too, `Reactions::requestGeneric`).
    fn maybe_fetch_live_bursts(&mut self) -> Result<(), ConnectSendError> {
        if self.session.stories.live_bursts != LiveBursts::Unasked {
            return Ok(());
        }
        self.live_story_request(
            LiveStoryPurpose::GetBursts,
            get_custom_emoji_reaction_animations,
        )?;
        self.session.stories.live_bursts = LiveBursts::Requested;
        Ok(())
    }

    /// The streamer's camera and screen for the engine's video
    /// subscriptions: an MTProto stream sends video parts only for
    /// subscribed endpoints. The streamer may be a channel.
    pub(crate) fn live_story_video_sources(&self, group_call_id: i32) -> Vec<GroupVideoSource> {
        let Some(streamer) = self
            .session
            .stories
            .live
            .as_ref()
            .filter(|live| live.group_call_id == group_call_id)
            .and_then(|live| live.streamer.as_ref())
        else {
            return Vec::new();
        };
        if streamer.is_current_user {
            return Vec::new();
        }
        let key = match streamer.participant_id {
            MessageSender::User { user_id } => user_id,
            MessageSender::Chat { chat_id } => chat_id,
        };
        [
            (&streamer.video_info, false),
            (&streamer.screen_sharing_video_info, true),
        ]
        .into_iter()
        .filter_map(|(info, screen)| info.as_ref().map(|info| (info, screen)))
        .filter(|(info, _)| !info.is_paused && !info.endpoint_id.is_empty())
        .filter_map(|(info, screen)| {
            let ssrc_groups: Vec<crate::calls::engine::GroupVideoSourceGroup> = info
                .source_groups
                .iter()
                .filter(|group| !group.source_ids.is_empty())
                .map(|group| crate::calls::engine::GroupVideoSourceGroup {
                    semantics: group.semantics.clone(),
                    ssrcs: group.source_ids.clone(),
                })
                .collect();
            (!ssrc_groups.is_empty()).then(|| GroupVideoSource {
                user_id: key,
                endpoint: info.endpoint_id.clone(),
                ssrc_groups,
                screen,
                audio_ssrc: streamer.audio_source_id.cast_unsigned(),
            })
        })
        .collect()
    }

    /// A left click on the Stars button (or the box's Send): `stars` more
    /// on the pending reaction, committed 5.5 seconds after the last tap.
    /// Refused on your own live story, past the cap, or without the
    /// balance (the note says why).
    pub fn add_live_story_reaction(
        &mut self,
        stars: i64,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || stars <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(group_call_id) = self
            .watched_live_call()
            .filter(|call| !call.can_be_managed)
            .map(|call| call.id)
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let max = self.session.live_max_stars();
        let short = self.session.live_needs_top_up(stars);
        let Some(live) = self.session.live_story_mut(group_call_id) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let pending = live.pending_reaction.map_or(0, |p| p.stars);
        let refusal = if pending.saturating_add(stars) > max {
            Some(Refusal::TooMany)
        } else if short {
            Some(Refusal::NotEnough)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            live.note = Some(refusal.message().to_string());
            return Err(ConnectSendError::InvalidRequest);
        }
        live.note = None;
        self.live_story_request(
            LiveStoryPurpose::AddPendingReaction {
                group_call_id,
                stars,
            },
            |extra| add_pending_live_story_reaction(extra, group_call_id, stars),
        )?;
        if let Some(live) = self.session.live_story_mut(group_call_id) {
            live.pending_reaction = Some(PendingLiveReaction::add(
                live.pending_reaction,
                stars,
                now_ms,
            ));
        }
        Ok(())
    }

    /// The toast's Undo: the pending Stars are dropped.
    pub fn undo_live_story_reaction(&mut self) -> Result<(), ConnectSendError> {
        let Some(live) = self.session.stories.live.as_mut() else {
            return Ok(());
        };
        if live.pending_reaction.take().is_none() {
            return Ok(());
        }
        let group_call_id = live.group_call_id;
        self.live_story_request(
            LiveStoryPurpose::RemovePendingReactions { group_call_id },
            |extra| remove_pending_live_story_reactions(extra, group_call_id),
        )
        .map(|_| ())
    }

    /// Commits the pending Stars once their Undo window is over. Returns
    /// whether anything was sent.
    pub fn live_story_reaction_tick(&mut self, now_ms: u64) -> bool {
        let Some(live) = self.session.stories.live.as_mut() else {
            return false;
        };
        if !live.pending_reaction.is_some_and(|p| p.due(now_ms)) {
            return false;
        }
        let Some(pending) = live.commit_reaction() else {
            return false;
        };
        let group_call_id = live.group_call_id;
        // tdesktop `addStars(from, stars, mine)` at send time: your Stars
        // fly up from the button as the reaction goes out.
        if let Some(sender) = self.session.live_story_sender()
            && let Some(live) = self.session.live_story_mut(group_call_id)
        {
            live.push_effect(sender, pending.stars);
        }
        if self
            .live_story_request(
                LiveStoryPurpose::CommitPendingReactions { group_call_id },
                |extra| commit_pending_live_story_reactions(extra, group_call_id),
            )
            .is_err()
            && let Some(live) = self.session.live_story_mut(group_call_id)
        {
            live.committed_stars = live.committed_stars.saturating_sub(pending.stars);
            live.note = Some(Refusal::NotEnough.message().to_string());
        }
        true
    }

    /// Sends a comment (Quill's field markup) with the Stars the viewer
    /// chose (0 on your own live story). Line feeds become spaces; only the
    /// entities a comment may carry go out (`comment_text`); an empty
    /// comment is refused.
    pub fn send_live_story_comment(
        &mut self,
        markup: &str,
        stars: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || stars < 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (text, entities) = comment_text(markup);
        let Some((group_call_id, manage, allowed)) = self
            .watched_live_call()
            .map(|call| (call.id, call.can_be_managed, call.are_messages_allowed))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if text.is_empty() || !allowed || stars > self.session.live_max_stars() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let stars = if manage { 0 } else { stars };
        // Stars waiting on a reaction (or on messages) are already spent.
        if self.session.live_needs_top_up(stars) {
            if let Some(live) = self.session.live_story_mut(group_call_id) {
                live.note = Some(Refusal::NotEnough.message().to_string());
            }
            return Err(ConnectSendError::InvalidRequest);
        }
        self.live_story_request(LiveStoryPurpose::SendComment { group_call_id }, |extra| {
            send_live_story_comment(extra, group_call_id, &text, &entities, stars)
        })
    }

    /// "Send message as...": who your comments are sent as.
    pub fn set_live_story_sender(
        &mut self,
        sender: MessageSender,
    ) -> Result<RequestId, ConnectSendError> {
        let Some(group_call_id) = self.watched_live_call().map(|call| call.id) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let offered = self
            .session
            .stories
            .live
            .as_ref()
            .and_then(|live| live.message_senders.as_ref())
            .is_some_and(|senders| senders.iter().any(|s| s.sender == sender));
        if !offered {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.live_story_request(
            LiveStoryPurpose::SetMessageSender { group_call_id },
            |extra| set_live_story_message_sender(extra, group_call_id, sender),
        )?;
        // Shown at once; `updateGroupCall` confirms.
        if let Some(call) = self.session.calls.story_stream.as_mut() {
            call.message_sender_id = Some(sender);
        }
        Ok(extra)
    }

    /// Deletes a comment, or every comment of its sender, as the delete
    /// box chose (tdesktop `Messages::deleteConfirmed`; banning is not
    /// offered here).
    pub fn delete_live_story_comment(
        &mut self,
        message_id: i32,
        report_spam: bool,
        all_from: Option<MessageSender>,
    ) -> Result<RequestId, ConnectSendError> {
        let Some(group_call_id) = self.watched_live_call().map(|call| call.id) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let purpose = LiveStoryPurpose::DeleteMessages { group_call_id };
        let extra = match all_from {
            Some(sender) => self.live_story_request(purpose, |extra| {
                delete_group_call_messages_by_sender(extra, group_call_id, sender, report_spam)
            })?,
            None => self.live_story_request(purpose, |extra| {
                delete_group_call_messages(extra, group_call_id, &[message_id], report_spam)
            })?,
        };
        // tdesktop removes them at once.
        if let Some(call) = self.session.calls.story_stream.as_mut() {
            call.messages.retain(|m| match all_from {
                Some(sender) => m.sender_id != sender,
                None => m.message_id != message_id,
            });
        }
        self.session
            .after_live_story_messages_deleted(group_call_id);
        Ok(extra)
    }

    /// The viewer left the live story: pending Stars are dropped (tdesktop
    /// `undoScheduledPaidOnDestroy`) and its stream is left. The current
    /// call, if any, stays.
    pub fn leave_live_story(&mut self) -> Result<(), ConnectSendError> {
        let Some(live) = self.session.stories.live.take() else {
            return Ok(());
        };
        if self
            .session
            .calls
            .pending_live_story_join
            .map(|j| j.group_call_id)
            == Some(live.group_call_id)
        {
            self.session.calls.pending_live_story_join = None;
        }
        let group_call_id = live.group_call_id;
        if live.pending_reaction.is_some() {
            let extra = self.session.request(
                RequestPurpose::Stories(StoriesPurpose::Live(
                    LiveStoryPurpose::RemovePendingReactions { group_call_id },
                )),
                None,
            );
            let _ = self.send_json_request(
                extra,
                &remove_pending_live_story_reactions(extra, group_call_id),
            );
        }
        let Some(stream) = self
            .session
            .calls
            .story_stream
            .take_if(|call| call.id == group_call_id)
        else {
            return Ok(());
        };
        self.teardown_group_call_transport(group_call_id);
        // Not joined yet: TDLib has nothing to leave (and would answer an
        // error), so the slot is only dropped. A join on its way is
        // forgotten, so its late answer cannot land in a new slot for the
        // same story.
        let joining = self
            .session
            .requests
            .take_purpose(RequestPurpose::Calls(CallsPurpose::JoinVideoChat {
                group_call_id,
            }))
            .is_some();
        if !(stream.is_joined || stream.reconnecting || stream.need_rejoin || joining) {
            return Ok(());
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::LeaveGroupCall { group_call_id }),
            None,
        );
        self.send_json_request(extra, &leave_group_call(extra, group_call_id))
            .map(|_| ())
    }

    /// Whether a group call you joined (or are joining) belongs to
    /// `chat_id`, other than the live story's own.
    fn member_call_in_chat(&self, chat_id: i64) -> bool {
        let story = self.session.calls.story_stream.as_ref().map(|call| call.id);
        self.session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| {
                Some(call.id) != story && self.group_call_chat_id(call.id) == Some(chat_id)
            })
            && self.session.group_call_joined_or_joining(None)
    }

    /// A stream's picture (`LiveKey::Stream`), mode and quality are keyed
    /// by chat id, in the engine and in the UI, so a channel's live story
    /// and the same channel's live stream joined as a member would share
    /// them. The call you join wins, as tdesktop's group call takes over
    /// from a video stream: the story's transport goes and its slot is
    /// marked failed, which the viewer shows with Retry. Called before a
    /// member join of a call in `chat_id`; a story in another chat, and
    /// any 1:1 call, play beside it.
    pub(crate) fn yield_story_stream_to_chat(&mut self, chat_id: i64) {
        let Some(group_call_id) = self.session.calls.story_stream.as_ref().map(|call| call.id)
        else {
            return;
        };
        if self.group_call_chat_id(group_call_id) != Some(chat_id) {
            return;
        }
        self.teardown_group_call_transport(group_call_id);
        if let Some(call) = self.session.calls.story_stream.as_mut() {
            call.transport_ready = false;
            call.join_payload.clear();
            call.transport_error = Some(STORY_CHAT_TAKEN.to_string());
        }
    }

    /// The other half of [`Self::yield_story_stream_to_chat`]: a story
    /// opened while you are in a call of its chat does not join (no
    /// `joinLiveStory`, no engine instance) and shows failed. Returns
    /// whether it was refused.
    pub(crate) fn refuse_story_stream_in_member_chat(&mut self, group_call_id: i32) -> bool {
        let Some(chat_id) = self.group_call_chat_id(group_call_id) else {
            return false;
        };
        if !self.member_call_in_chat(chat_id) {
            return false;
        }
        if let Some(call) = self
            .session
            .calls
            .story_stream
            .as_mut()
            .filter(|call| call.id == group_call_id)
        {
            call.transport_error = Some(STORY_CHAT_TAKEN.to_string());
        }
        true
    }

    /// The feed the live story's picture comes from: the stream feed of
    /// the poster's chat for an MTProto or RTMP stream, else the
    /// streamer's camera (or screen) among the call's video.
    pub fn live_story_live_key(&self) -> Option<LiveKey> {
        let live = self.session.stories.live.as_ref()?;
        let call = self.watched_live_call()?;
        if call.stream_mode.is_stream() {
            return Some(LiveKey::Stream {
                chat_id: live.chat_id,
            });
        }
        let streamer = live.streamer.as_ref()?;
        let user_id = match streamer.participant_id {
            MessageSender::User { user_id } => user_id,
            MessageSender::Chat { chat_id } => chat_id,
        };
        let screen = streamer.video_info.is_none() && streamer.screen_sharing_video_info.is_some();
        Some(LiveKey::Video {
            call_id: call.id,
            user_id,
            screen,
        })
    }
}
