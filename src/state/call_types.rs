//! Call state types: 1:1 and group calls.
use super::*;

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
    pub transport: Option<TransportState>,
    pub transport_error: Option<String>,
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
    /// `set_camera_enabled`.
    pub camera_on: bool,
    /// Phase C2i: local screen-share send intent (UI toggle). The
    /// engine picks it up through `set_screen_share_enabled`; screen
    /// share replaces the camera (ntgcalls forbids camera+screen in
    /// Capture mode).
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
}

/// Phase C1: summary of the most recently ended call, driving the
/// call-end screen and the optional 1–5 rating card
/// (`callStateDiscarded.need_rating`, schema 1.8.67, line 7081).
#[derive(Debug, Clone)]
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
    /// `need_debug_information` / `need_log` are out of this slice
    /// (no media log exists; debug-info upload is C2) — kept so the
    /// end screen can say so honestly.
    pub need_debug_information: bool,
    pub need_log: bool,
    pub rating_sent: bool,
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

impl ActiveCall {
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
    /// UI "reconnecting" banner; set on `need_rejoin`, cleared when a
    /// rejoin is issued or a fresh joined `updateGroupCall` arrives.
    pub reconnecting: bool,
    /// Phase C2f: group-call rejoin attempts after `need_rejoin` —
    /// the C2d 1:1 reconnect discipline (max 3 attempts). Reset on a
    /// fresh joined `updateGroupCall` or when a new call takes the
    /// slot (`fresh()`).
    pub rejoin_attempts: usize,
    pub can_be_managed: bool,
    pub is_owned: bool,
    pub participant_count: i32,
    pub loaded_all_participants: bool,
    /// Sorted: recent speakers first (in `recent_speakers` order),
    /// then by `order` descending (lexicographic — schema: "The
    /// bigger is order, the higher is user in the list").
    pub participants: Vec<ParsedGroupCallParticipant>,
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
    /// completed (`ntg_connect` with the `joinVideoChat` answer). False
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
    /// The `Text` presentation answer from
    /// `startGroupCallScreenSharing`; consumed by the driver pump to
    /// finish the presentation handshake.
    pub screen_share_answer: String,
    /// `HttpUrl` from `getVideoChatInviteLink`, fetched on demand.
    pub invite_link: Option<String>,
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
    /// Blank tracked call for a newly seen call id. Participant state
    /// repopulates from updates.
    pub(crate) fn fresh(id: i32) -> Self {
        ActiveGroupCall {
            id,
            title: String::new(),
            is_video_chat: false,
            is_live_story: false,
            is_joined: false,
            need_rejoin: false,
            reconnecting: false,
            rejoin_attempts: 0,
            can_be_managed: false,
            is_owned: false,
            participant_count: 0,
            loaded_all_participants: false,
            participants: Vec::new(),
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
            screen_share_answer: String::new(),
            invite_link: None,
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
        }
    }

    /// Re-sort participants: recent speakers first (in reported
    /// order), then by `order` descending (lexicographic).
    pub(crate) fn sort_participants(&mut self) {
        let recent = self.recent_speaker_order.clone();
        self.participants.sort_by(|a, b| {
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
        CallSummary {
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
            rating_sent: false,
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

/// MED4: one-shot `getWebPageInstantView` answer — the URL plus the M2
/// `pageBlock*` content the IV reader renders.
#[derive(Debug, Clone)]
pub struct InstantViewPage {
    pub url: String,
    pub rich: RichMessageContent,
}
