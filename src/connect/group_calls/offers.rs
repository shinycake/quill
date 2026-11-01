//! Connect driver: a group call's join payload on its way. The engine makes
//! the instance and the payload off the UI thread, as tdesktop's
//! `emitJoinPayload` answers later (`GroupCall::rejoin`). Every join and the
//! screen share take the same steps: the request is registered, the engine
//! starts, and what goes out is decided when the payload lands, from the state
//! at that moment. An offer nobody wants any more, or one that failed, sends
//! nothing and leaves no instance behind.
use super::conference::input_ref;
use super::*;
use crate::calls::engine::{EngineError, GroupOffer, GroupOfferKey};
use crate::state::{ConferenceInput, ConferenceOrigin};
use crate::telegram::requests::create_group_call;

pub(super) enum OfferSend {
    /// `joinVideoChat` or `joinLiveStory`.
    Join,
    Rejoin {
        conference: Option<ConferenceInput>,
        orphaned_presentation: bool,
    },
    /// `joinGroupCall` or `createGroupCall`, from the pending conference.
    Conference,
    Screen,
}

/// The registered request waiting for its payload.
pub(crate) struct WaitingOffer {
    extra: RequestId,
    send: OfferSend,
}

impl<S: JsonSender> ConnectDriver<S> {
    pub(super) fn offer_waiting(&self, key: GroupOfferKey) -> Option<RequestId> {
        self.group_offers.get(&key).map(|waiting| waiting.extra)
    }

    /// Waits for what the engine `started`. An engine that already has the
    /// payload lands it here.
    pub(super) fn wait_for_offer(
        &mut self,
        key: GroupOfferKey,
        extra: RequestId,
        send: OfferSend,
        started: Result<(), EngineError>,
    ) {
        self.group_offers.insert(key, WaitingOffer { extra, send });
        match started {
            Ok(()) => {
                self.land_group_offers();
            }
            Err(err) => self.finish_offer(GroupOffer {
                key,
                payload: Err(err),
            }),
        }
    }

    /// Per poll: sends what the landed payloads were waiting for. Returns
    /// whether any landed.
    pub(crate) fn land_group_offers(&mut self) -> bool {
        if self.group_offers.is_empty() {
            return false;
        }
        let Some(engine) = self.call_engine.as_deref_mut() else {
            return false;
        };
        let landed = engine.take_group_offers();
        let any = !landed.is_empty();
        for offer in landed {
            self.finish_offer(offer);
        }
        any
    }

    fn finish_offer(&mut self, offer: GroupOffer) {
        let key = offer.key;
        let Some(WaitingOffer { extra, send }) = self.group_offers.remove(&key) else {
            return;
        };
        if !self.offer_wanted(key, extra, &send) {
            self.session.requests.take(extra);
            self.discard_offered_instance(key);
            return;
        }
        match offer.payload {
            Ok(payload) => {
                if matches!(key, GroupOfferKey::Call(_)) {
                    self.apply_offered_playback(key.id());
                }
                let mut params = GroupCallJoinParams {
                    audio_source_id: group_offer_audio_source_id(&payload),
                    payload,
                    is_muted: false,
                    is_my_video_enabled: false,
                };
                self.set_join_flags(&mut params, key.id(), &send);
                self.send_offer(key, extra, send, &params);
            }
            Err(err) => self.fail_offer(key, extra, send, err),
        }
    }

    /// Whether what the offer was for still stands.
    fn offer_wanted(&self, key: GroupOfferKey, extra: RequestId, send: &OfferSend) -> bool {
        if !self.chats_path_active() {
            return false;
        }
        let id = key.id();
        let calls = &self.session.calls;
        match send {
            OfferSend::Join => {
                calls.left_group_call != Some(id)
                    && calls.group_call(id).is_some_and(|call| !call.is_joined)
            }
            OfferSend::Rejoin { .. } => {
                calls.left_group_call != Some(id) && calls.group_call(id).is_some()
            }
            OfferSend::Conference => calls
                .pending_conference
                .as_ref()
                .is_some_and(|pending| pending.request == extra),
            OfferSend::Screen => calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.id == id && call.is_joined && call.screen_share_pending),
        }
    }

    fn discard_offered_instance(&mut self, key: GroupOfferKey) {
        let Some(engine) = self.call_engine.as_deref_mut() else {
            return;
        };
        let _ = match key {
            GroupOfferKey::Call(id) => engine.leave_group_call(id),
            GroupOfferKey::Screen(id) => engine.stop_screen_share(id),
        };
    }

    /// The output device chosen in settings, kept for connect.
    fn apply_offered_playback(&mut self, id: i32) {
        let muted = self.group_playback_muted.contains(&id);
        let output = self.selected_devices.1.clone();
        if let Some(engine) = self.call_engine.as_deref_mut()
            && let Err(err) = engine.set_group_playback(id, output.as_deref(), muted)
        {
            eprintln!("quill: group call output device not set: {err}");
        }
    }

    /// Mute and camera as they are now: a press during the wait counts.
    pub(super) fn set_join_flags(
        &self,
        params: &mut GroupCallJoinParams,
        id: i32,
        send: &OfferSend,
    ) {
        let calls = &self.session.calls;
        let tracked = calls.group_call(id);
        let is_story = calls.story_stream.as_ref().is_some_and(|c| c.id == id);
        let tracked_muted = tracked.is_some_and(|call| call.is_muted_self);
        let tracked_video = tracked.is_some_and(|call| call.is_my_video_enabled);
        (params.is_muted, params.is_my_video_enabled) = match send {
            OfferSend::Join if is_story => (true, tracked_video),
            OfferSend::Join
            | OfferSend::Rejoin {
                conference: None, ..
            } => (tracked_muted, tracked_video),
            OfferSend::Rejoin { .. } => (tracked_muted, false),
            OfferSend::Conference => (
                calls
                    .pending_conference
                    .as_ref()
                    .is_some_and(|pending| pending.muted),
                false,
            ),
            OfferSend::Screen => (false, false),
        };
    }

    /// The request for `params`, and what follows it.
    pub(super) fn send_offer(
        &mut self,
        key: GroupOfferKey,
        extra: RequestId,
        send: OfferSend,
        params: &GroupCallJoinParams,
    ) {
        let id = key.id();
        let json = match &send {
            OfferSend::Join
            | OfferSend::Rejoin {
                conference: None, ..
            } => self.group_join_request(extra, id, params),
            OfferSend::Rejoin {
                conference: Some(input),
                ..
            } => join_group_call(extra, &input_ref(input.clone()), params),
            OfferSend::Conference => {
                let origin = self
                    .session
                    .calls
                    .pending_conference
                    .as_ref()
                    .map(|pending| pending.origin.clone());
                match origin {
                    Some(ConferenceOrigin::Invitation {
                        chat_id,
                        message_id,
                    }) => join_group_call(
                        extra,
                        &InputGroupCallRef::Message {
                            chat_id,
                            message_id,
                        },
                        params,
                    ),
                    Some(ConferenceOrigin::Link { link }) => {
                        join_group_call(extra, &InputGroupCallRef::Link(link), params)
                    }
                    Some(ConferenceOrigin::Create { .. }) | None => {
                        create_group_call(extra, Some(params))
                    }
                }
            }
            OfferSend::Screen => {
                start_group_call_screen_sharing(extra, id, params.audio_source_id, &params.payload)
            }
        };
        if self.sender.send_json(&json).is_err() {
            self.session.requests.take(extra);
            self.discard_offered_instance(key);
            self.undo_offer(id, &send);
            return;
        }
        match send {
            OfferSend::Rejoin {
                orphaned_presentation,
                ..
            } => {
                if orphaned_presentation && let Some(engine) = self.call_engine.as_deref_mut() {
                    let _ = engine.stop_screen_share(id);
                }
                if let Some(call) = self.session.calls.group_call_mut(id) {
                    call.rejoin_attempts += 1;
                    call.reconnecting = false;
                }
            }
            OfferSend::Conference => self.forget_accepted_invite(),
            OfferSend::Join | OfferSend::Screen => {}
        }
    }

    /// An accepted invitation stays offered until its join goes out.
    fn forget_accepted_invite(&mut self) {
        let origin = self
            .session
            .calls
            .pending_conference
            .as_ref()
            .map(|pending| pending.origin.clone());
        if let Some(ConferenceOrigin::Invitation {
            chat_id,
            message_id,
        }) = origin
        {
            self.session.forget_conference_invite(
                crate::ids::ChatId(chat_id),
                &[crate::ids::MessageId(message_id)],
            );
        }
    }

    /// No payload came: nothing is sent, as a join without media would show
    /// you in a call you can't hear. The Join button, or the next rejoin
    /// attempt, starts over.
    fn fail_offer(
        &mut self,
        key: GroupOfferKey,
        extra: RequestId,
        send: OfferSend,
        err: EngineError,
    ) {
        self.session.requests.take(extra);
        self.discard_offered_instance(key);
        let id = key.id();
        if matches!(send, OfferSend::Conference) {
            self.session.calls.group_call_error = Some(format!("Could not join the call: {err}"));
        } else if let Some(call) = self.session.calls.group_call_mut(id) {
            call.transport_error = Some(err.to_string());
            if matches!(send, OfferSend::Rejoin { .. }) {
                call.rejoin_attempts += 1;
            }
        }
        self.undo_offer(id, &send);
    }

    /// Puts back what starting the offer changed.
    fn undo_offer(&mut self, id: i32, send: &OfferSend) {
        match send {
            OfferSend::Join => {}
            OfferSend::Rejoin { .. } => {
                if let Some(call) = self.session.calls.group_call_mut(id) {
                    call.reconnecting = true;
                }
            }
            OfferSend::Conference => {
                self.session.calls.pending_conference = None;
                self.forget_conference_crypto();
            }
            OfferSend::Screen => {
                if let Some(call) = self.session.calls.group_call_mut(id) {
                    call.screen_share_pending = false;
                }
            }
        }
    }

    /// Stops waiting for `key`; the caller has dropped it from the engine.
    pub(super) fn drop_group_offer(&mut self, key: GroupOfferKey) -> Option<RequestId> {
        let waiting = self.group_offers.remove(&key)?;
        self.session.requests.take(waiting.extra);
        Some(waiting.extra)
    }

    pub(super) fn drop_group_offers(&mut self, id: i32) {
        self.drop_group_offer(GroupOfferKey::Call(id));
        self.drop_group_offer(GroupOfferKey::Screen(id));
    }

    /// The engine that was making them is gone.
    pub(crate) fn forget_all_group_offers(&mut self) {
        for (_, waiting) in std::mem::take(&mut self.group_offers) {
            self.session.requests.take(waiting.extra);
        }
    }
}
