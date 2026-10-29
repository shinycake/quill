//! Connect driver: conference calls, the end-to-end encrypted group calls
//! that aren't bound to a chat (docs/decisions/tgcalls-conference-calls.md).
//! One is joined from a `messageGroupCall` invitation or a call link, or
//! created by "Add people" in a 1:1 call, which then ends with the
//! conference's link (tdesktop `startOrJoinConferenceCall`).
use super::*;
use crate::state::{ConferenceInput, ConferenceOrigin, ConferenceUpgrade};
use crate::telegram::envelope::{CallState, CallsPayload, Envelope, EnvelopePayload};
use crate::telegram::group_call_crypto::GroupCallCrypto;
use crate::telegram::requests::create_group_call;
use std::sync::Arc;

/// The engine key of a conference call whose join hasn't answered yet:
/// the call's id comes with the answer, and the instance is renamed then
/// (`CallEngine::rekey_group_call`). TDLib's group call ids are positive.
pub(crate) const PENDING_CONFERENCE_KEY: i32 = -1;

/// A conference call's frame encryption and the engine key it is under.
pub(crate) struct ConferenceCrypto {
    pub(crate) key: i32,
    pub(crate) crypto: Arc<GroupCallCrypto>,
}

fn input_ref(input: ConferenceInput) -> InputGroupCallRef {
    match input {
        ConferenceInput::Link(link) => InputGroupCallRef::Link(link),
        ConferenceInput::Message {
            chat_id,
            message_id,
        } => InputGroupCallRef::Message {
            chat_id,
            message_id,
        },
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Accept a `messageGroupCall` invitation: `joinGroupCall` with
    /// `inputGroupCallMessage` ("Use joinGroupCall to accept the call",
    /// tdesktop `acceptConferenceInvite`).
    pub fn accept_group_call_invitation(
        &mut self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        self.join_conference(
            ConferenceOrigin::Invitation {
                chat_id,
                message_id,
            },
            false,
        )
    }

    /// Join a conference call by its link (`internalLinkTypeGroupCall`).
    pub fn join_conference_link(&mut self, link: String) -> Result<RequestId, ConnectSendError> {
        self.join_conference(ConferenceOrigin::Link { link }, false)
    }

    /// "Add people" in an established 1:1 call: a new conference call is
    /// created with you in it; once it has its link the 1:1 call ends
    /// with it, so the other side follows, and `invite` are invited (or
    /// the link is shown, `share_link`). tdesktop toasts "wait until the
    /// call is established" before then, and offers "Add people" only
    /// when the other side's app takes conference calls.
    pub fn upgrade_call_to_conference(
        &mut self,
        invite: Vec<i64>,
        share_link: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let upgrade = match &self.session.calls.active_call {
            Some(call)
                if matches!(call.state, CallState::Ready)
                    && call
                        .ready
                        .as_ref()
                        .is_some_and(|ready| ready.is_group_call_supported) =>
            {
                ConferenceUpgrade {
                    call_id: call.id,
                    peer_user_id: call.user_id,
                    is_video: call.is_video,
                    invite: invite
                        .into_iter()
                        .filter(|user_id| *user_id != call.user_id)
                        .collect(),
                    share_link,
                }
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let muted = self
            .session
            .calls
            .active_call
            .as_ref()
            .is_some_and(|call| call.muted);
        self.join_conference(
            ConferenceOrigin::Create {
                upgrade: Some(upgrade),
            },
            muted,
        )
    }

    /// `joinGroupCall` or `createGroupCall` for a conference call. The
    /// engine instance is made first, under [`PENDING_CONFERENCE_KEY`], so
    /// the request carries its tgcalls offer. Refused without an engine
    /// (no media would flow), while another join is in flight, while you
    /// are in a group call, and during a 1:1 call unless it is the one
    /// being taken into the conference.
    fn join_conference(
        &mut self,
        origin: ConferenceOrigin,
        muted: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.has_call_engine() {
            return Err(ConnectSendError::CallsUnavailable);
        }
        if self.session.calls.pending_conference.is_some()
            || self
                .session
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.is_joined || call.is_conference())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let migrating = match &origin {
            ConferenceOrigin::Create {
                upgrade: Some(upgrade),
            } => Some(upgrade.call_id),
            _ => None,
        };
        let in_call = self.session.calls.active_call.as_ref().is_some_and(|call| {
            !matches!(
                call.state,
                CallState::HangingUp | CallState::Discarded { .. } | CallState::Error { .. }
            )
        });
        let active_call = self.session.calls.active_call.as_ref().map(|call| call.id);
        if in_call && (migrating.is_none() || migrating != active_call) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(link) = self.sender.crypto_link() else {
            return Err(ConnectSendError::CallsUnavailable);
        };
        // A call only looked at gives way (tdesktop's single current call).
        if self.session.calls.active_group_call.is_some() {
            self.drop_group_call_locally();
        }
        self.forget_conference_crypto();
        let crypto = Arc::new(GroupCallCrypto::new(link));
        let params = match self.conference_join_params(PENDING_CONFERENCE_KEY, &crypto, muted) {
            Ok(params) => params,
            Err(err) => {
                self.session.calls.group_call_error =
                    Some(format!("Could not join the call: {err}"));
                return Err(ConnectSendError::CallsUnavailable);
            }
        };
        let extra = self.session.request(RequestPurpose::JoinConference, None);
        let json = match &origin {
            ConferenceOrigin::Create { .. } => create_group_call(extra, Some(&params)),
            ConferenceOrigin::Invitation {
                chat_id,
                message_id,
            } => join_group_call(
                extra,
                &InputGroupCallRef::Message {
                    chat_id: *chat_id,
                    message_id: *message_id,
                },
                &params,
            ),
            ConferenceOrigin::Link { link } => {
                join_group_call(extra, &InputGroupCallRef::Link(link.clone()), &params)
            }
        };
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.leave_group_call(PENDING_CONFERENCE_KEY);
            }
            return Err(err);
        }
        self.conference_crypto = Some(ConferenceCrypto {
            key: PENDING_CONFERENCE_KEY,
            crypto,
        });
        self.session.begin_conference(extra, origin, muted);
        Ok(extra)
    }

    /// The engine instance for a conference call under `key`, and the join
    /// parameters carrying its offer. A conference call never starts with
    /// the camera on (tdesktop joins it with the 1:1 call's camera; Quill
    /// turns it on from the panel).
    fn conference_join_params(
        &mut self,
        key: i32,
        crypto: &Arc<GroupCallCrypto>,
        is_muted: bool,
    ) -> Result<GroupCallJoinParams, crate::calls::engine::EngineError> {
        let output = self.selected_devices.1.clone();
        let Some(engine) = self.call_engine.as_deref_mut() else {
            return Err(crate::calls::engine::EngineError::Unavailable);
        };
        let crypto: Arc<dyn crate::calls::engine::FrameCrypto> = crypto.clone();
        let payload = engine.create_conference_call(key, crypto)?;
        if let Err(err) = engine.set_group_playback(key, output.as_deref(), false) {
            eprintln!("quill: conference call output device not set: {err}");
        }
        Ok(GroupCallJoinParams {
            audio_source_id: group_offer_audio_source_id(&payload),
            payload,
            is_muted,
            is_my_video_enabled: false,
        })
    }

    /// A conference rejoin (`need_rejoin`): a fresh instance under the
    /// call's id, joined by its link (or how it was first reached).
    pub(super) fn conference_rejoin_request(
        &mut self,
        extra: RequestId,
        group_call_id: i32,
        input: ConferenceInput,
        is_muted: bool,
    ) -> String {
        let crypto = match &self.conference_crypto {
            Some(known) if known.key == group_call_id => known.crypto.clone(),
            _ => {
                let link = self
                    .sender
                    .crypto_link()
                    .unwrap_or_else(|| Arc::new(crate::telegram::group_call_crypto::RefusingLink));
                let crypto = Arc::new(GroupCallCrypto::new(link));
                crypto.bind(group_call_id);
                self.conference_crypto = Some(ConferenceCrypto {
                    key: group_call_id,
                    crypto: crypto.clone(),
                });
                crypto
            }
        };
        let params = match self.conference_join_params(group_call_id, &crypto, is_muted) {
            Ok(params) => params,
            Err(err) => {
                if let Some(call) = self.session.calls.group_call_mut(group_call_id) {
                    call.transport_error = Some(err.to_string());
                }
                let mut params = GroupCallJoinParams::honest_no_device();
                params.is_muted = is_muted;
                params
            }
        };
        join_group_call(extra, &input_ref(input), &params)
    }

    /// Before the reducer sees it: the answer to a conference join names
    /// the call, so the engine instance takes its id and frames start
    /// being encrypted for it (tdesktop `setCallId`). A refused join drops
    /// the instance.
    pub(crate) fn note_conference_answer(&mut self, envelope: &Envelope) {
        let Some(extra) = envelope.extra else {
            return;
        };
        let ours = self
            .session
            .calls
            .pending_conference
            .as_ref()
            .is_some_and(|pending| pending.request == extra);
        if !ours {
            return;
        }
        match &envelope.payload {
            EnvelopePayload::Calls(CallsPayload::GroupCallInfo { group_call_id, .. }) => {
                let group_call_id = *group_call_id;
                if let Some(engine) = self.call_engine.as_deref_mut()
                    && let Err(err) = engine.rekey_group_call(PENDING_CONFERENCE_KEY, group_call_id)
                {
                    eprintln!("quill: conference call instance not renamed: {err}");
                }
                if let Some(known) = self.conference_crypto.as_mut()
                    && known.key == PENDING_CONFERENCE_KEY
                {
                    known.key = group_call_id;
                    known.crypto.bind(group_call_id);
                }
            }
            EnvelopePayload::Error(_) => {
                if let Some(engine) = self.call_engine.as_deref_mut() {
                    let _ = engine.leave_group_call(PENDING_CONFERENCE_KEY);
                }
                self.forget_conference_crypto();
            }
            _ => {}
        }
    }

    /// The call `group_call_id` is over here: frames still in flight are
    /// dropped (tdesktop `clearCallId`).
    pub(super) fn forget_conference_call(&mut self, group_call_id: i32) {
        if self
            .conference_crypto
            .as_ref()
            .is_some_and(|known| known.key == group_call_id)
        {
            self.forget_conference_crypto();
        }
    }

    fn forget_conference_crypto(&mut self) {
        if let Some(known) = self.conference_crypto.take() {
            known.crypto.unbind();
            if known.key == PENDING_CONFERENCE_KEY
                && let Some(engine) = self.call_engine.as_deref_mut()
            {
                let _ = engine.leave_group_call(PENDING_CONFERENCE_KEY);
            }
        }
    }

    /// Who sends each audio ssrc, for the engine's decryption.
    pub(super) fn sync_conference_ssrc_owners(&mut self, group_call_id: i32) {
        let owners = match self.session.calls.group_call(group_call_id) {
            Some(call) if call.is_conference() => crate::calls::engine::conference_ssrc_owners(
                &call.participants,
                call.loaded_all_participants,
            ),
            _ => return,
        };
        if let Some(engine) = self.call_engine.as_deref_mut() {
            engine.set_group_ssrc_owners(group_call_id, &owners);
        }
    }

    /// After every update: a conference made from a 1:1 call, once it has
    /// its link, ends the 1:1 call with it and invites the people picked
    /// (tdesktop `startedConferenceReady`: `finishConferenceInvitations`,
    /// then `destroyCurrentCall` with the slug). The other side, told by
    /// `callDiscardReasonUpgradeToGroupCall`, joins by the link
    /// (`finishByMigration`).
    pub(crate) fn maybe_continue_conference(&mut self) -> Result<(), ConnectSendError> {
        if let Some(upgrade) = self.session.calls.call_upgrade.take()
            && let Err(err) = self.join_conference(
                ConferenceOrigin::Link {
                    link: upgrade.invite_link,
                },
                upgrade.muted,
            )
        {
            // Already in a conference (the side that made it), or no
            // engine: the 1:1 call simply ended.
            let _ = err;
        }
        let ready = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| !call.invite_link.is_empty())
            .and_then(|call| {
                let upgrade = call.conference.as_ref()?.upgrade.clone()?;
                Some((call.invite_link.clone(), upgrade))
            });
        let Some((invite_link, upgrade)) = ready else {
            return Ok(());
        };
        if let Some(conference) = self
            .session
            .calls
            .active_group_call
            .as_mut()
            .and_then(|call| call.conference.as_mut())
        {
            conference.upgrade = None;
            conference.migrated_from = Some(upgrade.call_id);
            conference.link_open = upgrade.share_link && upgrade.invite.is_empty();
        }
        let mut result = Ok(());
        for user_id in upgrade.invite {
            if let Err(err) = self.invite_group_call_participant(user_id) {
                result = Err(err);
            }
        }
        let still_in_call = self
            .session
            .calls
            .active_call
            .as_ref()
            .is_some_and(|call| call.id == upgrade.call_id);
        if still_in_call && let Err(err) = self.discard_call_with_link(&invite_link) {
            result = Err(err);
        }
        result
    }
}
