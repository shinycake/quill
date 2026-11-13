//! Call state types: 1:1 and group calls.
use super::*;
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
    /// How far the call got and TDLib's last word on it, as one value:
    /// `callStateReady`'s record exists exactly from the Ready update on.
    /// Read the state through [`Self::state`] and the phase to show through
    /// [`Self::phase`], which says `HangingUp` while a hang-up is in flight.
    pub progress: CallProgress,
    /// What this side did to the call: answered it, or hung it up (the
    /// `discardCall` in flight owns the `HangingUp` phase).
    pub action: CallAction,
    /// When the call was created (outgoing) or first rang (incoming) —
    /// drives the "ringing" time on the overlay.
    pub started_at: Instant,
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
    /// What you send as video (see [`LocalVideo`]). Initialized from
    /// `is_video` — a video call starts with the camera on, a voice
    /// call with it off.
    pub video: LocalVideo,
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
    /// Why your microphone, level tap or shared screen stopped, for the
    /// call window. It lives and dies with the call.
    pub media_note: Option<String>,
}

/// How a call ended, from its terminal update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallEnd {
    /// Hung up, missed or any other discard.
    Ended,
    /// An outgoing call the peer declined before it connected: tdesktop
    /// shows "line busy" with Redial / Cancel.
    Declined,
    /// TDLib's `callStateError`.
    Failed,
}

/// How many `CallProblem` constructors the rating editor offers.
pub const CALL_PROBLEM_COUNT: usize = 9;

/// The rating detail being drafted for one call: the stars and the problems
/// picked (index-aligned with the UI's `CALL_PROBLEMS`). It lives in the
/// call's own rating stage, so it goes with the call's summary and comes
/// back when the send fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatingDraft {
    pub stars: i32,
    pub problems: [bool; CALL_PROBLEM_COUNT],
}

impl RatingDraft {
    pub fn new(stars: i32) -> Self {
        Self {
            stars,
            problems: [false; CALL_PROBLEM_COUNT],
        }
    }
}

/// The rating card of an ended call. The card is drawn while it is
/// `Offered`, `Editing` or `Failed`; `Sending` leaves on the answer to that
/// request, with the draft it sent. `NotNeeded`: TDLib did not ask for a
/// rating (`need_rating` false).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallRating {
    NotNeeded,
    Offered,
    /// A star was tapped: the editor is open on this draft.
    Editing(RatingDraft),
    Sending {
        request: RequestId,
        draft: RatingDraft,
    },
    /// The rating was sent and answered: the end state that keeps the card
    /// from coming back.
    Rated,
    /// The send failed: the card says why and the editor is back on the
    /// draft that was sent.
    Failed {
        error: String,
        draft: RatingDraft,
    },
}

impl CallRating {
    /// The card still asks for a rating (nothing is in flight or done).
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Offered | Self::Editing(_) | Self::Failed { .. })
    }

    /// The draft the editor shows.
    pub fn draft(&self) -> Option<&RatingDraft> {
        match self {
            Self::Editing(draft) | Self::Failed { draft, .. } => Some(draft),
            _ => None,
        }
    }

    pub fn draft_mut(&mut self) -> Option<&mut RatingDraft> {
        match self {
            Self::Editing(draft) | Self::Failed { draft, .. } => Some(draft),
            _ => None,
        }
    }

    /// The error line of a failed send.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// Whether `request` is the send in flight.
    pub fn sends(&self, request: RequestId) -> bool {
        matches!(self, Self::Sending { request: sent, .. } if *sent == request)
    }
}

/// One upload from the call-end card (diagnostics, call log). `Sending`
/// leaves on the answer to that request, so an answer for another call's
/// upload never lands here. `NotNeeded`: TDLib did not ask for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallUpload {
    NotNeeded,
    Idle,
    Sending(RequestId),
    /// The upload was answered; nothing leaves this stage.
    Uploaded,
    Failed(String),
}

impl CallUpload {
    /// The button may start the upload (nothing is in flight or done).
    pub fn can_send(&self) -> bool {
        matches!(self, Self::Idle | Self::Failed(_))
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error),
            Self::NotNeeded | Self::Idle | Self::Sending(_) | Self::Uploaded => None,
        }
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
    /// How the call ended, for the end screen and its sound.
    pub end: CallEnd,
    /// The rating card's stage (`need_rating`); one value so the card comes
    /// back when the send fails.
    pub rating: CallRating,
    /// `sendCallDebugInformation` upload stage (`need_debug_information`).
    pub debug_information: CallUpload,
    /// Phase C2i: `sendCallLog` upload stage (`need_log` from
    /// `callStateDiscarded`, schema 1.8.67 :7080).
    pub log: CallUpload,
    pub final_transport: Option<TransportState>,
    pub reconnect_attempts: usize,
    pub muted: bool,
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
    ) -> Self {
        Self {
            id,
            user_id,
            is_outgoing,
            is_video,
            progress: CallProgress::first(state, Instant::now()),
            action: CallAction::Idle,
            started_at: Instant::now(),
            transport: CallTransport::default(),
            signaling_queue: Vec::new(),
            signaling_dropped: 0,
            muted: false,
            video: if is_video {
                LocalVideo::Camera
            } else {
                LocalVideo::Off
            },
            remote_video: RemoteVideoState::Inactive,
            remote_screen: RemoteVideoState::Inactive,
            remote_audio_muted: false,
            remote_battery_low: false,
            signal_bars: None,
            media_note: None,
        }
    }

    /// The phase to show and act on: `HangingUp` while the `discardCall` is
    /// in flight, else TDLib's.
    pub fn phase(&self) -> CallState {
        if self.action.hang_up().is_some() {
            CallState::HangingUp
        } else {
            self.state().clone()
        }
    }

    /// The camera is what you send.
    pub fn camera_on(&self) -> bool {
        self.video == LocalVideo::Camera
    }

    /// The screen is what you send.
    pub fn screen_sharing(&self) -> bool {
        self.video == LocalVideo::Screen
    }

    /// The camera turns on (replacing a screen share) or off (a share
    /// stays).
    pub fn set_camera_on(&mut self, on: bool) {
        if on {
            self.video = LocalVideo::Camera;
        } else if self.camera_on() {
            self.video = LocalVideo::Off;
        }
    }

    /// The screen share turns on (replacing the camera) or off (the camera
    /// stays).
    pub fn set_screen_sharing(&mut self, on: bool) {
        if on {
            self.video = LocalVideo::Screen;
        } else if self.screen_sharing() {
            self.video = LocalVideo::Off;
        }
    }

    /// Seconds since `callStateReady` (the billable call duration).
    pub fn connected_secs(&self) -> i64 {
        self.progress
            .ready()
            .map(|ready| ready.since.elapsed().as_secs() as i64)
            .unwrap_or(0)
    }

    /// The parameters `callStateReady` carried (the key emojis, the group
    /// call support).
    pub fn ready_params(&self) -> Option<&ReadyParams> {
        self.progress.ready().map(|ready| &ready.params)
    }

    /// TDLib's last state of the call (the phase to show is
    /// [`Self::phase`]).
    pub fn state(&self) -> &CallState {
        self.progress.state()
    }
}

/// Your camera in a group call: off, on, or on and paused. Paused is only
/// ever a camera that is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MyVideo {
    #[default]
    Off,
    On,
    Paused,
}

impl MyVideo {
    /// What `groupCall.is_my_video_enabled` / `is_my_video_paused` say
    /// (paused with the camera off is off).
    pub fn from_server(enabled: bool, paused: bool) -> Self {
        match (enabled, paused) {
            (false, _) => Self::Off,
            (true, false) => Self::On,
            (true, true) => Self::Paused,
        }
    }

    pub fn is_enabled(self) -> bool {
        self != Self::Off
    }

    pub fn is_paused(self) -> bool {
        self == Self::Paused
    }

    /// The camera is on and sending frames.
    pub fn is_live(self) -> bool {
        self == Self::On
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

/// The video you send in a 1:1 call: a call has one video track, so the
/// camera and a screen share replace each other. The UI toggles are the
/// intent; the engine picks it up through `set_camera` and
/// `set_screen_share_enabled`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LocalVideo {
    #[default]
    Off,
    Camera,
    Screen,
}

/// 10830, `updateGroupCallVerificationState` 10836).
#[derive(Debug, Clone)]
pub struct ActiveGroupCall {
    pub id: i32,
    pub title: String,
    pub is_video_chat: bool,
    pub is_live_story: bool,
    pub is_joined: bool,
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
    /// The "Show more" request for the next participants.
    pub paging: crate::state::ParticipantPaging,
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
    /// Your camera in the call (`is_my_video_enabled` and
    /// `is_my_video_paused` of the group call, as one value).
    pub my_video: MyVideo,
    pub can_enable_video: bool,
    pub mute_new_participants: bool,
    pub can_toggle_mute_new_participants: bool,
    pub verification: Option<GroupCallVerificationState>,
    /// The native group transport: the `joinVideoChat` answer the driver
    /// pump connects, the completed handshake, or its failure (offer,
    /// connect, subscribe). The join itself is never blocked by these.
    pub transport: GroupTransport,
    /// Screen-share presentation: the `startGroupCallScreenSharing`
    /// handshake, then the connected presentation (paused while your shared
    /// window is hidden: the driver sent
    /// `toggleGroupCallScreenSharingIsPaused`, and your own tile shows it at
    /// once, before the participant update), or the last share's failure.
    /// A failed share leaves the call working.
    pub screen_share: GroupScreenShare,
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
    pub rtmp: Option<crate::calls::rtmp_start::RtmpInfo>,
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
    /// "Join as" choices from `getVideoChatAvailableParticipants`: asked
    /// once per tracked call (a failure leaves the picker out, the join
    /// goes ahead as yourself); empty when there are no alternatives.
    pub join_as_options: crate::state::fetch::Fetch<Vec<MessageSender>>,
    /// The mute toggles of participants that went out, each under its
    /// request; removed by that request's answer or error, and gone with
    /// the call.
    pub mute_toggles: std::collections::HashMap<MessageSender, RequestId>,
    /// The invitations "Add People" sent into this conference, until their
    /// toast is dismissed (`crate::state::conference_add_people`); gone
    /// with the call, so they never outlive it.
    pub invite_results: Option<crate::state::ConferenceInviteResults>,
    /// The identity the next join uses; `None` joins as yourself.
    pub join_as: Option<MessageSender>,
    /// How the joined call carries media, from the engine after the native
    /// connect (`CallEngine::group_call_mode`), and for a stream mode what
    /// the call shows: connecting, nobody broadcasting, live, ended or not
    /// playable on this build. Stream modes get their segments from TDLib
    /// (`calls::engine::StreamFeeder`).
    pub media: crate::state::GroupMedia,
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
        !matches!(self.rejoin, GroupCallRejoin::Idle | GroupCallRejoin::Left)
    }

    /// The join or its transport failed, or the rejoins are spent: only a
    /// manual retry brings it back.
    pub fn join_failed(&self) -> bool {
        self.transport.error().is_some() || self.rejoin == GroupCallRejoin::GaveUp
    }

    /// The window offers Rejoin: the connection was lost, or the join
    /// itself failed (a failed camera on a connected call is not one).
    pub fn offers_rejoin(&self) -> bool {
        self.reconnecting() || (self.join_failed() && !self.transport.is_connected())
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
            rejoin: GroupCallRejoin::Idle,
            can_be_managed: false,
            is_owned: false,
            participant_count: 0,
            loaded_all_participants: false,
            paging: crate::state::ParticipantPaging::Idle,
            participants: ParticipantList::default(),
            recent_speaker_order: Vec::new(),
            is_muted_self: false,
            my_video: MyVideo::Off,
            can_enable_video: false,
            mute_new_participants: false,
            can_toggle_mute_new_participants: false,
            verification: None,
            transport: GroupTransport::NoTransport,
            screen_share: GroupScreenShare::Off,
            share_links: Default::default(),
            is_rtmp_stream: false,
            scheduled_start_date: 0,
            enabled_start_notification: false,
            rtmp: None,
            can_send_messages: false,
            are_messages_allowed: false,
            can_toggle_are_messages_allowed: false,
            can_delete_messages: false,
            messages: Vec::new(),
            record_duration: 0,
            is_video_recorded: false,
            join_as_options: Default::default(),
            mute_toggles: Default::default(),
            invite_results: None,
            join_as: None,
            media: Default::default(),
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
    /// An outgoing call the peer declined before it connected.
    pub fn busy(&self) -> bool {
        self.end == CallEnd::Declined
    }

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
        let end = match &call.state {
            CallState::Discarded {
                reason: crate::telegram::envelope::CallDiscardReason::Declined,
                ..
            } if call.is_outgoing && duration_secs == 0 => CallEnd::Declined,
            CallState::Error { .. } => CallEnd::Failed,
            _ => CallEnd::Ended,
        };
        CallSummary {
            end,
            call_id: call.id,
            user_id: call.user_id,
            is_outgoing: call.is_outgoing,
            is_video: call.is_video,
            duration_secs,
            had_audio,
            end_line,
            rating: if need_rating {
                CallRating::Offered
            } else {
                CallRating::NotNeeded
            },
            debug_information: if need_debug_information {
                CallUpload::Idle
            } else {
                CallUpload::NotNeeded
            },
            log: if need_log {
                CallUpload::Idle
            } else {
                CallUpload::NotNeeded
            },
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
/// A `getWebPageInstantView` request in flight (`RequestPurpose` stays
/// `Copy`, so the URL rides `MessagesState::instant_view_requests` keyed by
/// the `RequestId`). It leaves with its answer or its error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstantViewAsk {
    pub url: String,
    /// Sent while the agent UI driver injected input
    /// (`guard::injecting`): the browser fallback opens as part of that step.
    pub from_agent: bool,
}

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
