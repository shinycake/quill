//! Connect driver: starting, accepting, swapping and ending one-to-one calls.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C1: `createCall` for a user. Gated on a known non-bot
    /// user (like `createNewSecretChat`) and on no call already being
    /// active. Phase C1b: `is_video: true` starts video-call
    /// *signaling* — media transport is still Phase C2, so the call
    /// carries no audio or video; the UI says so. The `callId` answer
    /// starts tracking the outgoing call; its states arrive as
    /// `updateCall`.
    pub fn start_call(
        &mut self,
        user_id: i64,
        is_video: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !self.has_call_engine() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.active_call.is_some()
            || self.session.requests.pending.values().any(|pending| {
                matches!(
                    pending.purpose,
                    RequestPurpose::Calls(CallsPurpose::CreateCall { .. })
                )
            })
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let user = self.session.user(user_id);
        let is_bot = user.is_some_and(|u| u.is_bot);
        let is_self = self.session.my_user_id.is_some_and(|me| me == user_id);
        if user.is_none() || is_bot || is_self {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request_for_user(
            RequestPurpose::Calls(CallsPurpose::CreateCall { is_video }),
            user_id,
        );
        let protocol = self.engine_protocol_json();
        if let Err(err) = self.sender.send_json(&create_call_with_protocol(
            extra, user_id, is_video, &protocol,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C1: `acceptCall` for the tracked incoming call. Requires an
    /// incoming `Pending` call — answering a call that already moved on
    /// is rejected here, not sent.
    pub fn accept_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !self.has_call_engine() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = match &self.session.active_call {
            Some(call) if !call.is_outgoing && matches!(call.state, CallState::Pending { .. }) => {
                call.id
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::AcceptCall, None);
        let protocol = self.engine_protocol_json();
        if let Err(err) = self
            .sender
            .send_json(&accept_call_with_protocol(extra, call_id, &protocol))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(engine) = self.call_engine.as_deref_mut() {
            // TDLib remains the call-state source of truth; this only marks
            // the already-tracked engine-side call accepted.
            let _ = engine.accept_call(call_id);
        }
        Ok(extra)
    }

    /// Phase C1: `discardCall` for the tracked call (decline an
    /// incoming call / hang up an active one). Marks the call
    /// `HangingUp` optimistically; the discard states arrive as
    /// `updateCall`. `duration` is the connected time in seconds.
    pub fn discard_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (call_id, duration_secs, is_video) = match &self.session.active_call {
            Some(call) => (call.id, call.connected_secs() as i32, call.is_video),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::DiscardCall, None);
        if let Err(err) = self.sender.send_json(&discard_call_request(
            extra,
            call_id,
            false,
            duration_secs,
            is_video,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(call) = self.session.active_call.as_mut() {
            call.state = CallState::HangingUp;
        }
        if let Some(engine) = self.call_engine.as_deref_mut() {
            // Prompt teardown; the later terminal updateCall repeats this
            // idempotently through `pump_call_engine`.
            let _ = engine.hangup(call_id);
        }
        Ok(extra)
    }

    /// Swap prompt: decline the pending incoming call as busy.
    /// Clears the prompt; the caller's terminal update is a no-op
    /// afterwards.
    pub fn decline_swap_call(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (call_id, _user_id, is_video) = self
            .session
            .call_swap_pending
            .take()
            .ok_or(ConnectSendError::InvalidRequest)?;
        let extra = self.session.request(RequestPurpose::DiscardCall, None);
        if let Err(err) = self
            .sender
            .send_json(&discard_call_request(extra, call_id, false, 0, is_video))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Swap prompt: end the current call and answer the pending
    /// incoming one. `discardCall` for the active call goes out
    /// immediately; `acceptCall` for the pending call fires from the
    /// driver pump once the terminal updateCall lands (TDLib allows a
    /// single active call, so the accept must wait for the discard).
    pub fn accept_swap_call(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (pending_id, _user_id, is_video) = self
            .session
            .call_swap_pending
            .take()
            .ok_or(ConnectSendError::InvalidRequest)?;
        self.session.call_swap_accept_queued = Some((pending_id, is_video));
        if self.session.active_call.is_some() {
            self.discard_call().map(|_| ())
        } else {
            self.maybe_accept_queued_swap()
        }
    }

    /// Driver pump: fire the queued post-swap `acceptCall` once no
    /// call is active. Called from `ingest` next to
    /// `maybe_decline_busy_calls`. If the caller hung up meanwhile,
    /// the server rejects the accept and the update stream records it.
    pub(crate) fn maybe_accept_queued_swap(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || self.session.active_call.is_some() {
            return Ok(());
        }
        let Some((call_id, _is_video)) = self.session.call_swap_accept_queued.take() else {
            return Ok(());
        };
        let extra = self.session.request(RequestPurpose::AcceptCall, None);
        let protocol = self.engine_protocol_json();
        if let Err(err) = self
            .sender
            .send_json(&accept_call_with_protocol(extra, call_id, &protocol))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(engine) = self.call_engine.as_deref_mut() {
            let _ = engine.accept_call(call_id);
        }
        Ok(())
    }

    /// Phase C2c: real mute through the native engine. When an available
    /// engine is installed and the transport exists, the engine is
    /// called first and its error propagates *without* flipping the
    /// session flag; otherwise the flag is stored (it applies to the
    /// transport on connect).
    pub fn set_call_muted(&mut self, muted: bool) -> Result<(), EngineError> {
        let Some(call) = self.session.active_call.as_mut() else {
            return Err(EngineError::NoActiveCall);
        };
        if call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            // Engine first: a failed native call must not flip the flag.
            engine.set_muted(call.id, muted)?;
        }
        call.muted = muted;
        Ok(())
    }

    /// Phase C2c: pick the (microphone, speaker) device ids. `None`
    /// means the engine default. The pair is always stored; it is
    /// forwarded to the native engine only when a transport is already
    /// connected, and a failed forward propagates before the stored
    /// selection changes.
    pub fn select_call_devices(
        &mut self,
        mic: Option<String>,
        speaker: Option<String>,
    ) -> Result<(), EngineError> {
        let selection = (mic, speaker);
        if let Some(call) = self.session.active_call.as_ref()
            && call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
        {
            engine.select_devices(call.id, selection.0.as_deref(), selection.1.as_deref())?;
        }
        self.selected_devices = selection;
        Ok(())
    }

    /// Phase C1: `sendCallRating` for the last ended call (the 1–5
    /// rating card, `callStateDiscarded.need_rating`). Marks the summary
    /// so the card can show "Thanks" while the `ok` confirms.
    /// Phase C2i: `sendCallRating` with problems + comment (schema
    /// 1.8.67 :14234). `problems` are `CallProblem` constructor names
    /// (`callProblemEcho`, …).
    pub fn send_call_rating(
        &mut self,
        rating: i32,
        comment: &str,
        problems: &[&str],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = match &self.session.call_summary {
            Some(summary) if summary.need_rating && !summary.rating_sent => summary.call_id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::SendCallRating, None);
        if let Err(err) = self.sender.send_json(&send_call_rating_detail(
            extra, call_id, rating, comment, problems,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(summary) = self.session.call_summary.as_mut() {
            summary.rating_sent = true;
        }
        Ok(extra)
    }
}
