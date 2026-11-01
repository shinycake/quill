//! Connect driver: starting, joining, rejoining and leaving group calls.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C3a / C2h: `createVideoChat` — start a voice chat on a
    /// group or channel (schema 1.8.67, :14256). Signaling only: the
    /// chat-bound creation path. The `groupCallId` answer queues a
    /// `getGroupCall` fetch; live state arrives as `updateGroupCall`.
    /// `start_date`: Unix timestamp, 0 = start immediately; otherwise
    /// at least 10s and at most 8 days in the future (schema). Empty
    /// title falls back to the chat title (schema).
    pub fn start_video_chat(
        &mut self,
        chat_id: i64,
        title: String,
        start_date: i64,
    ) -> Result<RequestId, ConnectSendError> {
        self.create_video_chat_request(chat_id, title, start_date, false)
    }

    /// `createVideoChat`, shared by an ordinary start and "Stream with..."
    /// (`is_rtmp_stream`).
    pub(super) fn create_video_chat_request(
        &mut self,
        chat_id: i64,
        title: String,
        start_date: i64,
        is_rtmp_stream: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_group_or_channel = self.session.chats.get(&chat_id).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. }
            )
        });
        if !is_group_or_channel {
            return Err(ConnectSendError::InvalidRequest);
        }
        // A 1:1 call being hung up no longer counts: the UI ends it right
        // before creating (tdesktop `createGroupCall` →
        // `destroyCurrentCall`).
        use crate::telegram::envelope::CallState;
        let in_call = self.session.calls.active_call.as_ref().is_some_and(|call| {
            !matches!(
                call.state,
                CallState::HangingUp | CallState::Discarded { .. } | CallState::Error { .. }
            )
        });
        if self.session.calls.active_group_call.is_some() || in_call {
            return Err(ConnectSendError::InvalidRequest);
        }
        let title = title.trim().to_string();
        if title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Schema :14256 — scheduled start must be ≥10s and ≤8d out.
        let start_date = if start_date == 0 {
            0
        } else {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if !(now + 10..=now + 8 * 86400).contains(&start_date) {
                return Err(ConnectSendError::InvalidRequest);
            }
            start_date.min(i32::MAX as i64) as i32
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::CreateVideoChat { chat_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&create_video_chat(
            extra,
            chat_id,
            &title,
            start_date,
            is_rtmp_stream,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `joinVideoChat` for a chat-bound voice chat (schema
    /// 1.8.67, :14292). Joins as self; the TDLib `Text` answer is stored
    /// on the tracked call and consumed by the driver pump to finish the
    /// native handshake.
    /// Phase C2g: the native group transport is created first so the
    /// join carries the real tgcalls join payload as its payload, with
    /// `audio_source_id` read from it, so the request goes out when the
    /// payload lands (`offers.rs`). Without an engine nothing is sent
    /// (`CallsUnavailable`): a join would show you in the call while you
    /// hear and see nothing.
    pub fn join_video_chat(&mut self, group_call_id: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.has_call_engine() {
            return Err(ConnectSendError::CallsUnavailable);
        }
        // Never join while a 1:1 call is active. A tracked group call is
        // fine to join when it is the same call and not yet joined (the
        // normal flow: getGroupCall creates the unjoined tracker, then the
        // overlay's Join button calls this). Reject a different tracked call
        // or one already joined.
        if self.session.calls.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.id == group_call_id && !call.is_joined)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Joining it again: its joined updates count once more.
        if self.session.calls.left_group_call == Some(group_call_id) {
            self.session.calls.left_group_call = None;
        }
        Ok(self.start_group_join(group_call_id))
    }

    /// One join at a time: a second press is the same join.
    fn start_group_join(&mut self, group_call_id: i32) -> RequestId {
        if let Some(extra) = self.offer_waiting(GroupOfferKey::Call(group_call_id)) {
            return extra;
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::JoinVideoChat { group_call_id }),
            None,
        );
        self.start_group_offer(group_call_id, extra, OfferSend::Join);
        extra
    }

    /// Select the schema method for initial joins, overlay retries, and reconnects.
    pub(super) fn group_join_request(
        &self,
        extra: RequestId,
        id: i32,
        params: &GroupCallJoinParams,
    ) -> String {
        if self
            .session
            .calls
            .story_stream
            .as_ref()
            .is_some_and(|call| call.id == id)
        {
            join_live_story_request(extra, id, params)
        } else {
            let as_ref = self
                .session
                .calls
                .active_group_call
                .as_ref()
                .and_then(|call| call.join_as)
                .map(|sender| match sender {
                    MessageSender::User { user_id } => MessageSenderRef::User(user_id),
                    MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
                });
            let hash = crate::call_link::PendingCallLink::hash_for(
                self.session.calls.call_link.as_ref(),
                id,
            );
            join_video_chat(extra, id, as_ref.as_ref(), params, hash)
        }
    }

    /// The chat a tracked voice chat belongs to (`groupCall` carries no
    /// chat id; the chat's `video_chat` association does). A live story
    /// has no `video_chat`: its poster's chat is the one.
    pub(crate) fn group_call_chat_id(&self, group_call_id: i32) -> Option<i64> {
        self.session
            .chats
            .values()
            .find(|chat| {
                chat.video_chat
                    .as_ref()
                    .is_some_and(|video_chat| video_chat.group_call_id == group_call_id)
            })
            .map(|chat| chat.id.0)
            .or_else(|| {
                self.session
                    .stories
                    .live
                    .as_ref()
                    .filter(|live| live.group_call_id == group_call_id)
                    .map(|live| live.chat_id)
            })
    }

    /// `getVideoChatAvailableParticipants`, once per unjoined chat-bound
    /// call, so the join screen can offer "join as". Called from `ingest`.
    pub(crate) fn maybe_fetch_join_as(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if !call.is_joined && !call.join_as_requested && call.is_video_chat => {
                call.id
            }
            _ => return Ok(()),
        };
        let Some(chat_id) = self.group_call_chat_id(group_call_id) else {
            return Ok(());
        };
        if let Some(call) = self.session.calls.active_group_call.as_mut() {
            call.join_as_requested = true;
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::GetVideoChatAvailableParticipants {
                group_call_id,
            }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&get_video_chat_available_participants(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Pick the identity to join as and remember it as the chat default
    /// (`setVideoChatDefaultParticipant`), as tdesktop's join-as box does.
    pub fn choose_group_call_join_as(
        &mut self,
        sender: MessageSender,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if !call.is_joined => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let chat_id = self
            .group_call_chat_id(group_call_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        self.session.set_group_call_join_as(Some(sender));
        let sender_ref = match sender {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::SetVideoChatDefaultParticipant { group_call_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&set_video_chat_default_participant(
            extra,
            chat_id,
            &sender_ref,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// stories-live-play: the story viewer's "Join live" sets
    /// `pending_live_story_join`; once the `getGroupCall` answer has
    /// created the unjoined tracker, issue `join_video_chat` for it.
    /// The intent is kept while the `getGroupCall` request is in flight
    /// and dropped when the request completes without a tracker (fetch
    /// failed — a later tap can retry). The group-call overlay's Join
    /// button stays the manual fallback either way.
    pub(crate) fn maybe_join_live_story(&mut self) -> Result<(), ConnectSendError> {
        self.maybe_fetch_live_story_extras()?;
        self.maybe_resolve_live_story_custom_emoji()?;
        let intent = match self.session.calls.pending_live_story_join {
            Some(intent) => intent,
            None => return Ok(()),
        };
        let ready = self
            .session
            .calls
            .story_stream
            .as_ref()
            .is_some_and(|call| call.id == intent.group_call_id && !call.is_joined);
        if ready {
            self.session.calls.pending_live_story_join = None;
            if !self.refuse_story_stream_in_member_chat(intent.group_call_id) {
                self.join_story_stream(intent.group_call_id)?;
            }
            return Ok(());
        }
        // The `getGroupCall` request finished without producing a
        // tracker: the fetch failed. Drop the intent so a later tap can
        // retry; while the request is still in flight the intent is kept,
        // so the join can't fire before the tracker exists and can't be
        // lost to an early ingest either.
        if self.session.requests.get(intent.request).is_none() {
            self.session.calls.pending_live_story_join = None;
        }
        Ok(())
    }

    /// `joinLiveStory` for the story stream: muted, beside any call you
    /// are in (tdesktop's video stream is never the current call).
    fn join_story_stream(&mut self, group_call_id: i32) -> Result<RequestId, ConnectSendError> {
        if !self
            .session
            .calls
            .story_stream
            .as_ref()
            .is_some_and(|call| call.id == group_call_id && !call.is_joined)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(self.start_group_join(group_call_id))
    }

    /// Phase C3a: `getGroupCall` for a known call id (schema 1.8.67,
    /// :14274). Used to start tracking a voice chat found via a chat's
    /// `video_chat` affordance.
    pub fn fetch_group_call(&mut self, group_call_id: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::GetGroupCall { group_call_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&get_group_call(extra, group_call_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: drain `Session::group_call_fetch_queue` — `getGroupCall`
    /// for freshly created voice chats. Called from `ingest`.
    pub(crate) fn maybe_fetch_group_calls(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<i32> = std::mem::take(&mut self.session.calls.group_call_fetch_queue);
        for group_call_id in queued {
            if let Err(err) = self.fetch_group_call(group_call_id) {
                // Best-effort: re-queue for the next ingest tick; the
                // `updateGroupCall` backstop still tracks the call.
                let _ = err;
                self.session
                    .calls
                    .group_call_fetch_queue
                    .push(group_call_id);
            }
        }
        Ok(())
    }

    /// Phase C2f: rejoin after `need_rejoin` (schema 1.8.67, line 7154
    /// docs: "user was kicked from the call because of network loss and
    /// the call needs to be rejoined"). Same attempt discipline as the
    /// C2d 1:1 reconnect: at most 3 attempts with identical join params
    /// (current self-mute state, honest no-device). A failed attempt
    /// re-arms `reconnecting` via the `JoinVideoChat` error arm so the
    /// driver's auto-rejoin retries; `manual` (the UI Rejoin button)
    /// resets the counter — explicit user intent starts the attempts
    /// over.
    pub fn rejoin_group_call(&mut self, manual: bool) -> Result<RequestId, ConnectSendError> {
        let Some(group_call_id) = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .map(|call| call.id)
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        self.rejoin_tracked_group_call(group_call_id, manual)
    }

    fn rejoin_tracked_group_call(
        &mut self,
        group_call_id: i32,
        manual: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.has_call_engine() {
            return Err(ConnectSendError::CallsUnavailable);
        }
        if let Some(extra) = self.offer_waiting(GroupOfferKey::Call(group_call_id)) {
            return Ok(extra);
        }
        if manual && let Some(call) = self.session.calls.group_call_mut(group_call_id) {
            call.rejoin_attempts = 0;
        }
        let conference = match self.session.calls.group_call(group_call_id) {
            Some(call)
                if call.reconnecting
                    && call.rejoin_attempts < crate::state::MAX_GROUP_CALL_REJOIN_ATTEMPTS =>
            {
                call.is_conference().then(|| call.conference_input())
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        // A conference call is rejoined by its link; without one known
        // there is nothing to rejoin with.
        if conference == Some(None) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if conference.is_some() {
            RequestPurpose::JoinConference
        } else {
            RequestPurpose::Calls(CallsPurpose::JoinVideoChat { group_call_id })
        };
        let extra = self.session.request(purpose, None);
        // Review fix: the new native context starts unconnected, and
        // `create_group_call` below replaces the old media entry — a
        // live presentation would be orphaned (still capturing on the
        // native side). Snapshot it before the entry is replaced.
        let mut orphaned_presentation = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.presentation_active(group_call_id));
        // A stream's pending parts are answered, then the old native call
        // stops, before the fresh context replaces it. Without the stop
        // the old instance keeps asking for parts that nobody serves. The
        // new connect starts a new feeder.
        self.drain_stream_feeder(group_call_id);
        if let Some(engine) = self.call_engine.as_deref_mut()
            && engine.leave_group_call(group_call_id).is_ok()
        {
            // The leave stopped the presentation first (privacy order).
            orphaned_presentation = false;
        }
        self.drop_group_offer(GroupOfferKey::Screen(group_call_id));
        if let Some(call) = self.session.calls.group_call_mut(group_call_id) {
            call.reconnecting = false;
            // Review fix: the fresh native context must run the join
            // handshake again — reset the transport gate, the stale
            // answer, and the stale screen-share state so the pump
            // doesn't drop the new `joinVideoChat` answer.
            call.transport_ready = false;
            call.join_payload.clear();
            call.screen_sharing = false;
            call.screen_share_pending = false;
            call.screen_share_answer.clear();
        }
        match conference.flatten() {
            Some(input) => {
                let send = OfferSend::Rejoin {
                    conference: Some(input),
                    orphaned_presentation,
                };
                self.start_conference_rejoin(extra, group_call_id, send);
            }
            None => {
                let send = OfferSend::Rejoin {
                    conference: None,
                    orphaned_presentation,
                };
                self.start_group_offer(group_call_id, extra, send);
            }
        }
        Ok(extra)
    }

    /// Phase C2f: auto-rejoin a dropped group call (`need_rejoin`) —
    /// one attempt per ingest tick while the tracked call still wants
    /// reconnecting and attempts remain (the `rejoin_group_call`
    /// guard caps at 3, the C2d discipline).
    pub(crate) fn maybe_auto_rejoin_group_call(&mut self) -> Result<(), ConnectSendError> {
        for group_call_id in self.session.calls.group_call_ids().into_iter().flatten() {
            let wants = self
                .session
                .calls
                .group_call(group_call_id)
                .is_some_and(|call| {
                    call.reconnecting
                        && call.rejoin_attempts < crate::state::MAX_GROUP_CALL_REJOIN_ATTEMPTS
                });
            if wants {
                let _ = self.rejoin_tracked_group_call(group_call_id, false);
            }
        }
        Ok(())
    }

    /// Phase C2g: start the native group context for a (re)join, so the
    /// request carries the real tgcalls offer. A call in no known chat has
    /// no context and goes out at once with the honest no-device params.
    /// The joins that call this refuse first without an engine.
    fn start_group_offer(&mut self, group_call_id: i32, extra: RequestId, send: OfferSend) {
        self.refresh_call_devices();
        // The native call key is the chat id; resolve it through the
        // chat's `video_chat` association (schema `groupCall` carries no
        // chat id).
        let chat_id = self.group_call_chat_id(group_call_id);
        let role = if self
            .session
            .calls
            .story_stream
            .as_ref()
            .is_some_and(|call| call.id == group_call_id)
        {
            GroupCallRole::Viewer
        } else {
            GroupCallRole::Member
        };
        if role == GroupCallRole::Member
            && let Some(chat_id) = chat_id
        {
            self.yield_story_stream_to_chat(chat_id);
        }
        let key = GroupOfferKey::Call(group_call_id);
        let started = match (chat_id, self.call_engine.as_deref_mut()) {
            (Some(chat_id), Some(engine)) if engine.is_available() => {
                Some(engine.create_group_call(group_call_id, chat_id, role))
            }
            _ => None,
        };
        match started {
            Some(started) => self.wait_for_offer(key, extra, send, started),
            None => {
                let mut params = GroupCallJoinParams::honest_no_device();
                self.set_join_flags(&mut params, group_call_id, &send);
                self.send_offer(key, extra, send, &params);
            }
        }
    }

    /// Phase C3a: `leaveGroupCall` (schema 1.8.67, :14458). Drops the
    /// tracked call; the `ok` confirms (state also handles the
    /// `!is_active` backstop).
    pub fn leave_group_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::LeaveGroupCall { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&leave_group_call(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.calls.left_group_call = Some(group_call_id);
        self.session.calls.invite_results = None;
        // Phase C2g: tear the native group transport down eagerly; the
        // pump's before/after backstop covers server-driven ends.
        self.teardown_group_call_transport(group_call_id);
        Ok(extra)
    }

    /// Drop the tracked group call without a request (one only looked
    /// at, or given up for another), and stop its native transport if it
    /// has one: the pump's before/after check only sees reducer updates.
    pub fn drop_group_call_locally(&mut self) {
        let tracked = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .map(|call| call.id);
        if let Some(group_call_id) = tracked {
            self.teardown_group_call_transport(group_call_id);
        }
        self.session.leave_group_call_local();
    }

    /// Phase C2g: drop the native transport and retained frames for one
    /// group call id. Unknown ids succeed (idempotent cleanup).
    /// A conference call's crypto is unbound first: frames in flight are
    /// answered at once instead of holding up the instance's destroy.
    pub(super) fn teardown_group_call_transport(&mut self, group_call_id: i32) {
        self.drop_group_offers(group_call_id);
        self.forget_conference_call(group_call_id);
        self.drain_stream_feeder(group_call_id);
        if let Some(engine) = self.call_engine.as_deref_mut() {
            let _ = engine.leave_group_call(group_call_id);
        }
        self.close_call_feeds(group_call_id);
        self.group_camera_state.remove(&group_call_id);
        self.group_mute_state.remove(&group_call_id);
        self.group_seen.remove(&group_call_id);
    }

    /// Phase C3a: `endGroupCall` (schema 1.8.67, :14461). Gated on
    /// `groupCall.can_be_managed` for a video chat, and on `is_owned` for
    /// a conference call ("requires groupCall.is_owned for group calls").
    pub fn end_group_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.can_be_managed || (call.is_conference() && call.is_owned) => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::EndGroupCall { group_call_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&end_group_call(extra, group_call_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.calls.left_group_call = Some(group_call_id);
        self.session.calls.invite_results = None;
        // Phase C2g: same native teardown as leaving; the server-driven
        // end also lands via the pump backstop.
        self.teardown_group_call_transport(group_call_id);
        Ok(extra)
    }
}

/// The driver going away (its account switched or signed out, the app
/// quitting) drops the engine and every instance with it: the crypto is
/// unbound first, so no media thread waits on TDLib during those destroys.
impl<S: JsonSender> Drop for ConnectDriver<S> {
    fn drop(&mut self) {
        if let Some(known) = self.conference_crypto.take() {
            known.crypto.unbind();
        }
    }
}
