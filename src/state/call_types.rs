//! Call state types: 1:1 and group calls.
use super::*;
use crate::calls::engine::ConnectParams;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Phase C1: the tracked live call (signaling only — no media
/// transport; real audio/video is the C2 libtgvoip spike). States
/// follow TDLib's `CallState` (schema 1.8.67, lines 7054–7086):
/// Pending → ExchangingKeys → Ready → HangingUp →
/// Discarded / Error.
#[derive(Debug, Clone)]
pub struct ActiveCall {
    pub id: i32,
    pub user_id: i64,
    pub is_outgoing: bool,
    pub is_video: bool,
    pub state: CallState,
    /// When the call was created (outgoing) or first rang (incoming) —
    /// drives the "ringing" time on the overlay.
    pub started_at: Instant,
    /// When `callStateReady` arrived — the call-duration clock starts
    /// here.
    pub ready_at: Option<Instant>,
    pub ready: Option<ReadyParams>,
    /// The media transport, its retained connect parameters and its
    /// error, as one value (see `CallTransport`).
    pub transport: CallTransport,
    /// Phase C2b: chunks from `updateNewCallSignalingData`; the engine now
    /// consumes them too, while this queue remains the honest diagnostic
    /// record. Capped at
    /// `MAX_QUEUED_SIGNALING_CHUNKS`; overflow is counted, not kept.
    pub signaling_queue: Vec<Vec<u8>>,
    pub signaling_dropped: usize,
    /// Phase C1b: local-only mute toggle state. Tracked but a no-op
    /// without media transport (C2) — the UI labels it honestly.
    pub muted: bool,
    /// Phase C2e: local camera intent (UI toggle). Initialized from
    /// `is_video` — a video call starts with the camera on, a voice
    /// call with it off. The engine picks it up through
    /// `set_camera`.
    pub camera_on: bool,
    /// Phase C2i: local screen-share send intent (UI toggle). The
    /// engine picks it up through `set_screen_share_enabled`; screen
    /// share replaces the camera (a 1:1 call has one video track).
    pub screen_sharing: bool,
    /// Phase C2e: peer camera state from the engine hook; `Inactive`
    /// until the first state callback arrives.
    pub remote_video: RemoteVideoState,
    /// Phase C2l: peer 1:1 screen-share state from the engine's screen
    /// hook; `Inactive` until the first state callback arrives. The
    /// screen-share tile renders only while this is not `Inactive`,
    /// so a late PLAYBACK+SCREEN frame arriving after the drain
    /// cannot repopulate a stale tile.
    pub remote_screen: RemoteVideoState,
    /// The peer's microphone is off (tdesktop `lng_call_microphone_off`),
    /// from the engine's MediaState signaling; `false` until it says so.
    pub remote_audio_muted: bool,
    /// The peer's battery is low (tdesktop `RemoteBatteryState::Low`,
    /// "{user}'s battery level is low"), from the engine; `false` until
    /// it says so. Quill never sends its own battery state, as tdesktop.
    pub remote_battery_low: bool,
    /// The connection quality, 0 to 4 bars (tdesktop `Calls::SignalBars`),
    /// from the engine; `None` until it reports.
    pub signal_bars: Option<u8>,
}

/// The rating card of an ended call. The card is drawn while it is
/// `Offered` or `Failed`; `Sending` leaves on the answer to that request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallRating {
    Offered,
    Sending(RequestId),
    Sent,
    Failed(String),
}

impl CallRating {
    /// The card still asks for a rating (nothing is in flight or done).
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Offered | Self::Failed(_))
    }
}

/// Phase C1: summary of the most recently ended call, driving the
/// call-end screen and the optional 1–5 rating card
/// (`callStateDiscarded.need_rating`, schema 1.8.67, line 7081).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSummary {
    pub call_id: i32,
    pub user_id: i64,
    pub is_outgoing: bool,
    pub is_video: bool,
    /// Seconds between `callStateReady` and the end (0 when the call
    /// never connected).
    pub duration_secs: i64,
    pub had_audio: bool,
    /// Human-readable end line (reason-aware).
    pub end_line: String,
    pub need_rating: bool,
    /// An outgoing call the peer declined before it connected: tdesktop
    /// shows "line busy" with Redial / Cancel.
    pub busy: bool,
    /// `need_debug_information` / `need_log` are out of this slice
    /// (no media log exists; debug-info upload is C2) — kept so the
    /// end screen can say so honestly.
    pub need_debug_information: bool,
    pub need_log: bool,
    /// The rating card's stage; one value so the card comes back when
    /// the send fails.
    pub rating: CallRating,
    pub debug_information_sent: bool,
    pub debug_information_error: Option<String>,
    /// Phase C2i: `sendCallLog` upload state (`need_log` from
    /// `callStateDiscarded`, schema 1.8.67 :7080).
    pub log_sent: bool,
    pub log_error: Option<String>,
    pub final_transport: Option<TransportState>,
    pub reconnect_attempts: usize,
    pub muted: bool,
}

/// The 1:1 call's media transport. One value, so a phase can never sit
/// next to a stale error and the retry budget lives with the call it
/// belongs to. Quill connects once per Ready call, retries the retained
/// parameters up to `MAX_CALL_RECONNECT_ATTEMPTS` times on a reported
/// failure, and a `Connected` report starts the budget over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTransport {
    /// Not connected yet. `start_error`: the engine could not start the
    /// call; the call stays signaling-only and still connects on Ready.
    NotStarted {
        start_error: Option<String>,
    },
    /// Connect issued, or the engine reports it is connecting.
    Connecting(CallLink),
    Connected(CallLink),
    /// A retry of the retained parameters is in flight.
    Reconnecting(CallLink),
    /// The engine closed the transport.
    Closed(CallLink),
    /// No transport. `link` is kept when one was connected, so a later
    /// engine failure report can still retry (or say the budget is spent).
    Failed {
        reason: String,
        link: Option<CallLink>,
    },
}

/// What a connected transport keeps for its reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CallLink {
    pub params: ConnectParams,
    /// Reconnects since the last `Connected` report.
    pub reconnect_attempts: usize,
}

/// Reconnects Quill tries on its own after the engine reports a failure.
pub const MAX_CALL_RECONNECT_ATTEMPTS: usize = 3;

impl Default for CallTransport {
    fn default() -> Self {
        Self::NotStarted { start_error: None }
    }
}

impl CallTransport {
    /// The phase the engine API and the UI speak; `None` before connect.
    pub fn state(&self) -> Option<TransportState> {
        match self {
            Self::NotStarted { .. } => None,
            Self::Connecting(_) => Some(TransportState::Connecting),
            Self::Connected(_) => Some(TransportState::Connected),
            Self::Reconnecting(_) => Some(TransportState::Reconnecting),
            Self::Closed(_) => Some(TransportState::Closed),
            Self::Failed { .. } => Some(TransportState::Failed),
        }
    }

    /// The error to show: a failed start before connect, or the failure.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::NotStarted { start_error } => start_error.as_deref(),
            Self::Failed { reason, .. } => Some(reason),
            _ => None,
        }
    }

    /// A connect was attempted (the engine owns a transport, or failed to).
    pub fn started(&self) -> bool {
        !matches!(self, Self::NotStarted { .. })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected(_))
    }

    pub fn link(&self) -> Option<&CallLink> {
        match self {
            Self::NotStarted { .. } => None,
            Self::Connecting(link)
            | Self::Connected(link)
            | Self::Reconnecting(link)
            | Self::Closed(link) => Some(link),
            Self::Failed { link, .. } => link.as_ref(),
        }
    }

    pub fn reconnect_attempts(&self) -> usize {
        self.link().map_or(0, |link| link.reconnect_attempts)
    }

    /// The engine could not start the call. Before connect it is the
    /// start error; a failed transport takes it as its reason; a live
    /// transport has outgrown it.
    pub fn record_start_error(&mut self, error: String) {
        match self {
            Self::NotStarted { start_error } => *start_error = Some(error),
            Self::Failed { reason, .. } => *reason = error,
            _ => {}
        }
    }

    /// A live phase the engine reported, carrying `link`. `Failed` is not
    /// one: a reported failure goes through the reconnect decision.
    pub fn reported(state: TransportState, link: CallLink) -> Option<Self> {
        match state {
            TransportState::Connecting => Some(Self::Connecting(link)),
            TransportState::Connected => Some(Self::Connected(CallLink {
                reconnect_attempts: 0,
                ..link
            })),
            TransportState::Reconnecting => Some(Self::Reconnecting(link)),
            TransportState::Closed => Some(Self::Closed(link)),
            TransportState::Failed => None,
        }
    }
}

impl ActiveCall {
    /// A call first seen now, in `state`: nothing exchanged yet, camera on
    /// for a video call.
    pub fn fresh(
        id: i32,
        user_id: i64,
        is_outgoing: bool,
        is_video: bool,
        state: CallState,
        ready: Option<ReadyParams>,
    ) -> Self {
        Self {
            id,
            user_id,
            is_outgoing,
            is_video,
            state,
            started_at: Instant::now(),
            ready_at: None,
            ready,
            transport: CallTransport::default(),
            signaling_queue: Vec::new(),
            signaling_dropped: 0,
            muted: false,
            camera_on: is_video,
            screen_sharing: false,
            remote_video: RemoteVideoState::Inactive,
            remote_screen: RemoteVideoState::Inactive,
            remote_audio_muted: false,
            remote_battery_low: false,
            signal_bars: None,
        }
    }

    /// Seconds since `callStateReady` (the billable call duration).
    pub fn connected_secs(&self) -> i64 {
        self.ready_at
            .map(|t| t.elapsed().as_secs() as i64)
            .unwrap_or(0)
    }
}

/// Phase C3a: E2E verification state of a group call
/// (`updateGroupCallVerificationState`, schema 1.8.67, line 10836).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCallVerificationState {
    pub generation: i32,
    pub emojis: Vec<String>,
}

/// Phase C3a: the tracked group call / voice chat (**signaling
/// only** — no media transport; real group audio/video is the C2
/// program). State follows TDLib's `groupCall` (schema 1.8.67, line
/// 7154) and its participant updates (`updateGroupCall` 10819,
/// `updateGroupCallParticipant` 10824, `updateGroupCallParticipants`
/// Phase C2f: group-call rejoins Quill tries on its own after
/// `need_rejoin` (the C2d 1:1 reconnect discipline). Past them only the
/// manual Rejoin is left.
pub const MAX_GROUP_CALL_REJOIN_ATTEMPTS: usize = 3;

/// 10830, `updateGroupCallVerificationState` 10836).
#[derive(Debug, Clone)]
pub struct ActiveGroupCall {
    pub id: i32,
    pub title: String,
    pub is_video_chat: bool,
    pub is_live_story: bool,
    pub is_joined: bool,
    /// `need_rejoin` arrived (kicked by network loss).
    pub need_rejoin: bool,
    /// Where Quill's own rejoin after a network loss stands (the
    /// C2d 1:1 reconnect discipline, at most
    /// `MAX_GROUP_CALL_REJOIN_ATTEMPTS`). Reset by a joined
    /// `updateGroupCall` without `need_rejoin`, and when a new call takes
    /// the slot (`fresh()`).
    pub rejoin: GroupCallRejoin,
    pub can_be_managed: bool,
    pub is_owned: bool,
    pub participant_count: i32,
    pub loaded_all_participants: bool,
    /// Sorted: recent speakers first (in `recent_speakers` order),
    /// then by `order` descending (lexicographic — schema: "The
    /// bigger is order, the higher is user in the list").
    pub participants: ParticipantList,
    /// Recent speakers as last reported by `updateGroupCall` (drives
    /// the participant ordering above).
    pub recent_speaker_order: Vec<MessageSender>,
    /// Phase C3a: local-only self mute. TDLib has no "mute self"
    /// request for group calls outside the join parameters, and there
    /// is no audio path yet (C2) — the UI labels this honestly as
    /// local-only. Sent as `is_muted` on (re)join.
    pub is_muted_self: bool,
    pub is_my_video_enabled: bool,
    pub is_my_video_paused: bool,
    pub can_enable_video: bool,
    pub mute_new_participants: bool,
    pub can_toggle_mute_new_participants: bool,
    pub verification: Option<GroupCallVerificationState>,
    /// The `Text` join payload returned by `joinVideoChat`; Phase C2g
    /// consumes it in the driver pump to finish the native group
    /// handshake.
    pub join_payload: String,
    /// Phase C2g: whether the native group transport handshake has
    /// completed (`connect_group_call` with the `joinVideoChat` answer). False
    /// when the join went out with the honest no-device params (no
    /// engine) or while the answer is still in flight.
    pub transport_ready: bool,
    /// Phase C2g: last native group-transport error (offer/connect/
    /// subscribe failures), mirroring the 1:1 call's `transport_error`.
    /// The join itself is never blocked by these.
    pub transport_error: Option<String>,
    /// Phase C2g: screen-share presentation state. `screen_share_pending`
    /// while the `startGroupCallScreenSharing` answer is in flight;
    /// `screen_sharing` once the presentation transport connected.
    pub screen_share_pending: bool,
    pub screen_sharing: bool,
    /// Your shared window is hidden, so the share is paused (the driver
    /// sent `toggleGroupCallScreenSharingIsPaused`). Your own tile shows
    /// the paused state at once, before the participant update.
    pub screen_share_paused: bool,
    /// The last screen share's failure (its offer, start, connect or
    /// stop). Kept apart from `transport_error`: a failed share leaves
    /// the call itself working.
    pub screen_share_error: Option<String>,
    /// The `Text` presentation answer from
    /// `startGroupCallScreenSharing`; consumed by the driver pump to
    /// finish the presentation handshake.
    pub screen_share_answer: String,
    /// Speaker and listener links from `getVideoChatInviteLink`,
    /// fetched on demand (`crate::calls::share_links`).
    pub share_links: crate::calls::share_links::VideoChatShareLinks,
    /// `groupCall.is_rtmp_stream`: an RTMP stream has no speaker link.
    pub is_rtmp_stream: bool,
    /// Phase C2h: `scheduled_start_date` of a not-yet-started video
    /// chat (0 = live or unknown). Drives the "starts in …" card.
    pub scheduled_start_date: i32,
    /// `enabled_start_notification` from `updateGroupCall` (schema
    /// 1.8.67, :7154) — "notify me when this scheduled chat starts".
    pub enabled_start_notification: bool,
    /// Phase C2h: `rtmpUrl` from `getVideoChatRtmpUrl` /
    /// `replaceVideoChatRtmpUrl`, fetched on demand by an admin.
    pub rtmp_url: Option<String>,
    pub rtmp_stream_key: Option<String>,
    /// Phase C2h: in-call chat flags (schema 1.8.67, lines
    /// 7147-7150).
    pub can_send_messages: bool,
    pub are_messages_allowed: bool,
    pub can_toggle_are_messages_allowed: bool,
    pub can_delete_messages: bool,
    /// Phase C2h: in-call chat messages — append-only live feed from
    /// `updateNewGroupCallMessage` (TDLib has no history getter for
    /// group-call messages, so only messages seen while joined are
    /// shown; capped).
    pub messages: Vec<ParsedGroupCallMessage>,
    /// Phase C2h: recording state from `updateGroupCall`
    /// (`record_duration` seconds, 0 = not recording).
    pub record_duration: i32,
    pub is_video_recorded: bool,
    /// "Join as" choices from `getVideoChatAvailableParticipants` (empty
    /// until loaded, or when the call has no alternatives to offer).
    pub join_as_options: Vec<MessageSender>,
    /// The identity the next join uses; `None` joins as yourself.
    pub join_as: Option<MessageSender>,
    /// The options request was sent (once per tracked call).
    pub join_as_requested: bool,
    /// How the joined call carries media, from the engine after the
    /// native connect (`CallEngine::group_call_mode`). Stream modes get their
    /// segments from TDLib (`calls::engine::StreamFeeder`).
    pub stream_mode: crate::calls::engine::GroupCallMode,
    /// What a stream-mode call shows: connecting, nobody broadcasting,
    /// live, ended or not playable on this build. Meaningful only while
    /// `stream_mode` is a stream mode.
    pub stream: crate::calls::engine::StreamState,
    /// `participant_count` as the stream's "{count} viewers" (tdesktop
    /// shows no listener rows for RTMP streams).
    pub viewer_count: u32,
    /// Live stories: the Stars each comment costs (0 = free).
    pub paid_message_star_count: i64,
    /// Live stories: who your comments are sent as.
    pub message_sender_id: Option<MessageSender>,
    /// `groupCall.invite_link`: a conference call's link.
    pub invite_link: String,
    /// `groupCall.unique_id` (0 until an update names it): an invitation
    /// to this call carries the same id.
    pub unique_id: i64,
    /// What a conference call keeps beyond its `groupCall` fields; `None`
    /// for a video chat, or a conference this session didn't join.
    pub conference: Option<ConferenceCall>,
}

/// Quill's own rejoin of a group call that TDLib (or a stream feeder)
/// says needs one. One value, so "a rejoin is due" and "the banner shows"
/// cannot drift apart: the driver sends a rejoin only from `Wanted`, and
/// a repeated `need_rejoin` while one is `InFlight` changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupCallRejoin {
    /// Joined, or never dropped.
    #[default]
    Idle,
    /// A rejoin is due; `attempts` were already spent on this drop.
    Wanted { attempts: usize },
    /// Attempt number `attempt` is being made: its offer is being built,
    /// or its join request (`request`) waits for TDLib.
    InFlight { request: RequestId, attempt: usize },
    /// Every attempt failed; only the manual Rejoin is left.
    GaveUp,
}

impl GroupCallRejoin {
    /// TDLib's `need_rejoin` (or a stream's own rejoin): a drop starts
    /// one rejoin series, and a series already under way goes on.
    pub fn want(&mut self) {
        if *self == GroupCallRejoin::Idle {
            *self = GroupCallRejoin::Wanted { attempts: 0 };
        }
    }

    /// The attempt the driver may start now, if any: from `Wanted` with
    /// attempts left.
    pub fn next_attempt(&self) -> Option<usize> {
        match *self {
            GroupCallRejoin::Wanted { attempts } if attempts < MAX_GROUP_CALL_REJOIN_ATTEMPTS => {
                Some(attempts + 1)
            }
            _ => None,
        }
    }

    pub fn in_flight(&self) -> Option<RequestId> {
        match *self {
            GroupCallRejoin::InFlight { request, .. } => Some(request),
            _ => None,
        }
    }

    /// Due or under way, with attempts left.
    pub fn is_rejoining(&self) -> bool {
        match *self {
            GroupCallRejoin::Wanted { .. } => self.next_attempt().is_some(),
            GroupCallRejoin::InFlight { .. } => true,
            GroupCallRejoin::Idle | GroupCallRejoin::GaveUp => false,
        }
    }

    /// The attempt behind `request` failed: the next one is due, or the
    /// series gave up. Whether `request` was this call's rejoin.
    pub fn attempt_failed(&mut self, request: RequestId) -> bool {
        match *self {
            GroupCallRejoin::InFlight {
                request: mine,
                attempt,
            } if mine == request => {
                *self = if attempt >= MAX_GROUP_CALL_REJOIN_ATTEMPTS {
                    GroupCallRejoin::GaveUp
                } else {
                    GroupCallRejoin::Wanted { attempts: attempt }
                };
                true
            }
            _ => false,
        }
    }

    /// TDLib answered the join behind `request`: the rejoin succeeded and
    /// the attempts start over. Any other answer (a first join, a stale
    /// rejoin) leaves the state alone.
    pub fn attempt_joined(&mut self, request: RequestId) {
        if self.in_flight() == Some(request) {
            *self = GroupCallRejoin::Idle;
        }
    }

    /// TDLib reports the call joined again after it was not: a join went
    /// through, so whatever was due or spent is over. A joined update that
    /// follows a joined one says nothing about a rejoin Quill started on
    /// its own (a stream's part fetch) and is not this.
    pub fn rejoined(&mut self) {
        *self = GroupCallRejoin::Idle;
    }

    /// The attempt behind `request` never reached TDLib (the send
    /// failed): it is not counted.
    pub fn attempt_unsent(&mut self, request: RequestId) {
        if let GroupCallRejoin::InFlight {
            request: mine,
            attempt,
        } = *self
            && mine == request
        {
            *self = GroupCallRejoin::Wanted {
                attempts: attempt.saturating_sub(1),
            };
        }
    }
}

/// stories-live-play: a pending "Join live" from the story viewer — the
/// group call id plus the `getGroupCall` request behind it, so the pump
/// can tell "still waiting" from "the fetch failed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveStoryJoinIntent {
    pub group_call_id: i32,
    pub request: RequestId,
}

impl ActiveGroupCall {
    /// An admin holds you muted: your own participant is muted for all and
    /// may not unmute itself. tdesktop's `MuteState::ForceMuted`; a raised
    /// hand (`RaisedHand`) is the same server state (`can_self_unmute`
    /// false) with a hand rating, so it is covered. Derived from the
    /// participant row, never stored.
    pub fn is_force_muted(&self) -> bool {
        self.participants
            .iter()
            .find(|p| p.is_current_user)
            .is_some_and(|me| me.is_muted_for_all_users && !me.can_unmute_self)
    }

    /// Dropped by the network and still being rejoined on its own: a
    /// rejoin is due or under way and attempts are left. Once the last one
    /// failed the call waits for the manual Rejoin.
    pub fn rejoining(&self) -> bool {
        self.rejoin.is_rejoining()
    }

    /// The "Connection lost" banner and the connecting self row: from
    /// `need_rejoin` until the rejoined `updateGroupCall`, the spent
    /// state included (the banner is the manual Rejoin).
    pub fn reconnecting(&self) -> bool {
        self.rejoin != GroupCallRejoin::Idle
    }

    /// The join or its transport failed, or the rejoins are spent: only a
    /// manual retry brings it back.
    pub fn join_failed(&self) -> bool {
        self.transport_error.is_some() || self.rejoin == GroupCallRejoin::GaveUp
    }

    /// The window offers Rejoin: the connection was lost, or the join
    /// itself failed (a failed camera on a connected call is not one).
    pub fn offers_rejoin(&self) -> bool {
        self.reconnecting() || (self.join_failed() && !self.transport_ready)
    }

    /// Blank tracked call for a newly seen call id. Participant state
    /// repopulates from updates.
    pub fn fresh(id: i32) -> Self {
        ActiveGroupCall {
            id,
            title: String::new(),
            is_video_chat: false,
            is_live_story: false,
            is_joined: false,
            need_rejoin: false,
            rejoin: GroupCallRejoin::Idle,
            can_be_managed: false,
            is_owned: false,
            participant_count: 0,
            loaded_all_participants: false,
            participants: ParticipantList::default(),
            recent_speaker_order: Vec::new(),
            is_muted_self: false,
            is_my_video_enabled: false,
            is_my_video_paused: false,
            can_enable_video: false,
            mute_new_participants: false,
            can_toggle_mute_new_participants: false,
            verification: None,
            join_payload: String::new(),
            transport_ready: false,
            transport_error: None,
            screen_share_pending: false,
            screen_sharing: false,
            screen_share_paused: false,
            screen_share_error: None,
            screen_share_answer: String::new(),
            share_links: Default::default(),
            is_rtmp_stream: false,
            scheduled_start_date: 0,
            enabled_start_notification: false,
            rtmp_url: None,
            rtmp_stream_key: None,
            can_send_messages: false,
            are_messages_allowed: false,
            can_toggle_are_messages_allowed: false,
            can_delete_messages: false,
            messages: Vec::new(),
            record_duration: 0,
            is_video_recorded: false,
            join_as_options: Vec::new(),
            join_as: None,
            join_as_requested: false,
            stream_mode: Default::default(),
            stream: Default::default(),
            viewer_count: 0,
            paid_message_star_count: 0,
            message_sender_id: None,
            invite_link: String::new(),
            unique_id: 0,
            conference: None,
        }
    }

    /// Re-sort participants: recent speakers first (in reported
    /// order), then by `order` descending (lexicographic).
    pub(crate) fn sort_participants(&mut self) {
        let recent = self.recent_speaker_order.clone();
        self.participants.edit().sort_by(|a, b| {
            let ra = recent.iter().position(|s| s == &a.participant_id);
            let rb = recent.iter().position(|s| s == &b.participant_id);
            match (ra, rb) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.order.cmp(&a.order),
            }
        });
    }
}

/// A group call's participants. [`Self::edit`] is the only way to change
/// them and moves the revision, so two equal revisions mean the same list.
#[derive(Debug, Clone, Default)]
pub struct ParticipantList {
    rows: Arc<Vec<ParsedGroupCallParticipant>>,
    revision: u64,
}

impl ParticipantList {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn edit(&mut self) -> &mut Vec<ParsedGroupCallParticipant> {
        self.revision = next_participants_revision();
        Arc::make_mut(&mut self.rows)
    }
}

impl std::ops::Deref for ParticipantList {
    type Target = [ParsedGroupCallParticipant];

    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}

impl From<Vec<ParsedGroupCallParticipant>> for ParticipantList {
    fn from(rows: Vec<ParsedGroupCallParticipant>) -> Self {
        Self {
            rows: Arc::new(rows),
            revision: next_participants_revision(),
        }
    }
}

fn next_participants_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl CallSummary {
    /// Build the end screen from a terminal `updateCall`. `duration_secs`
    /// is the connected time (0 when the call never reached `Ready`).
    pub(crate) fn from_terminal(call: &ParsedCall, duration_secs: i64, had_audio: bool) -> Self {
        let (end_line, need_rating, need_debug_information, need_log) = match &call.state {
            CallState::Discarded {
                reason,
                need_rating,
                need_debug_information,
                need_log,
            } => (
                reason.summary(call.is_outgoing),
                *need_rating,
                *need_debug_information,
                *need_log,
            ),
            CallState::Error { code } => {
                // TDLib reports a missed outgoing call whose timeout
                // expired with error code 4005000; the raw message text
                // is never stored (it can contain secrets), so any other
                // code renders as the code only.
                let line = if *code == 4005000 {
                    "No answer — the call timed out".to_string()
                } else {
                    format!("Call failed (error {code})")
                };
                (line, false, false, false)
            }
            _ => ("Call ended".to_string(), false, false, false),
        };
        let busy = call.is_outgoing
            && duration_secs == 0
            && matches!(
                call.state,
                CallState::Discarded {
                    reason: crate::telegram::envelope::CallDiscardReason::Declined,
                    ..
                }
            );
        CallSummary {
            busy,
            call_id: call.id,
            user_id: call.user_id,
            is_outgoing: call.is_outgoing,
            is_video: call.is_video,
            duration_secs,
            had_audio,
            end_line,
            need_rating,
            need_debug_information,
            need_log,
            rating: CallRating::Offered,
            debug_information_sent: false,
            debug_information_error: None,
            log_sent: false,
            log_error: None,
            final_transport: None,
            reconnect_attempts: 0,
            muted: false,
        }
    }
}

/// Cap for the honest signaling queue (C1: no consumer yet).
pub(crate) const MAX_QUEUED_SIGNALING_CHUNKS: usize = 32;

/// A link whose `getWebPageInstantView` failed (TDLib 404: no Instant View
/// for the page): the window that asked opens it in the browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstantViewFallback {
    /// The chat window that asked; `None` is the main window.
    pub window: Option<ChatId>,
    pub url: String,
    /// The request came from an agent's step (`guard::with_injecting`): the
    /// browser opens as part of that step.
    pub from_agent: bool,
}

/// MED4: one-shot `getWebPageInstantView` answer — the URL plus the M2
/// `pageBlock*` content the IV reader renders.
#[derive(Debug, Clone)]
pub struct InstantViewPage {
    pub url: String,
    pub rich: RichMessageContent,
}
