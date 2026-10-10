//! One-to-one calls, group calls, recent calls and call privacy: the `calls` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct CallsState {
    /// Phase C1: the tracked live call, if any. **Signaling only** —
    /// TDLib transports no audio/video (official clients use
    /// libtgvoip); real media transport is the C2 spike.
    pub active_call: Option<ActiveCall>,
    /// Phase C1: summary of the most recently ended call, driving the
    /// call-end screen and the optional 1–5 rating card
    /// (`callStateDiscarded.need_rating`).
    pub summary: Option<CallSummary>,
    /// Phase C1: last async call-request error (e.g. `createCall`
    /// rejected), shown on the call overlay and cleared when
    /// dismissed. Never a secret.
    pub error: Option<String>,
    /// Phase C3a: last async group-call request error (e.g.
    /// `joinVideoChat` rejected), shown on the group-call overlay and
    /// cleared when dismissed. Never a secret.
    pub group_call_error: Option<String>,
    /// Phase C1: incoming calls that arrived while another call was
    /// active — the driver discards them (busy) via `discardCall`.
    /// Entries are `(call_id, user_id, is_video)` so the decline
    /// reports the actual call kind rather than a hardcoded one.
    pub busy_decline_queue: Vec<(i32, i64, bool)>,
    /// Phase C2i: incoming calls auto-declined while busy, kept as
    /// `(user_id, is_video)` so the UI can say so honestly instead of
    /// declining silently. Drained by the UI banner.
    pub busy_declined: Vec<(i64, bool)>,
    /// Swap prompt: the first incoming call that arrived while another
    /// call was active, awaiting the user's decision — `(call_id,
    /// user_id, is_video)`. Further incoming calls while the prompt is
    /// open go to `call_busy_decline_queue` (auto-declined busy).
    pub swap_pending: Option<(i32, i64, bool)>,
    /// Swap prompt: the user chose "end current & answer" — the
    /// pending incoming call's `(call_id, is_video)`, accepted by the
    /// driver once the active call's terminal update lands (TDLib
    /// allows a single active call, so `acceptCall` waits for the
    /// discard to complete).
    pub swap_accept_queued: Option<(i32, bool)>,
    /// Phase C2i: "who can call me"
    /// (`userPrivacySettingAllowCalls`, schema 1.8.67 :9006).
    pub privacy_allow_calls: Option<PrivacyWho>,
    /// Phase C2i: peer-to-peer calls
    /// (`userPrivacySettingAllowPeerToPeerCalls`, schema 1.8.67 :9009).
    pub privacy_p2p: Option<PrivacyWho>,
    /// A privacy get/set round-trip is in flight (see
    /// `call_privacy_pending` — fetch sends two gets, so this clears
    /// only when the last response lands).
    pub privacy_loading: bool,
    /// Outstanding call-privacy get/set round-trips.
    pub privacy_pending: u8,
    /// The last privacy get/set failed.
    pub privacy_error: bool,
    /// Phase C2i: local call preferences (confirm-before-calling,
    /// less-data), persisted via `settings::CallPrefs`. The driver
    /// loads them at startup; the UI saves on toggle.
    pub prefs: CallPrefs,
    /// Phase C2i: recent calls from `searchCallMessages` (server-side
    /// history, schema 1.8.67 :11903) for the Recent-calls tab, newest
    /// first.
    pub recent_calls: Vec<ParsedMessage>,
    /// `next_offset` from the last `foundMessages` page; empty starts
    /// (or restarts) the list.
    pub recent_calls_offset: String,
    /// A `searchCallMessages` page is in flight.
    pub recent_calls_loading: bool,
    /// The last `searchCallMessages` request failed.
    pub recent_calls_error: bool,
    /// A `deleteAllCallMessages` request is in flight.
    pub recent_calls_clearing: bool,
    /// Phase C3a: the tracked group call / voice chat, if any.
    /// **Signaling only** — TDLib transports no audio/video; the
    /// `joinVideoChat` response payload is stored (`join_payload`) and
    /// never consumed (real media transport is the C2 program).
    pub active_group_call: Option<ActiveGroupCall>,
    /// Phase C3a: group-call ids whose full `groupCall` still needs a
    /// `getGroupCall` fetch (queued from the `createVideoChat`
    /// `groupCallId` answer). Drained by the driver.
    pub group_call_fetch_queue: Vec<i32>,
    /// stories-live-play: the story viewer's "Join live" asked for this
    /// group call; the driver issues `join_video_chat` once the
    /// `getGroupCall` answer has created the unjoined tracker.
    pub pending_live_story_join: Option<LiveStoryJoinIntent>,
}

impl CallsState {
    pub(crate) fn new() -> Self {
        Self {
            active_call: None,
            summary: None,
            error: None,
            group_call_error: None,
            busy_decline_queue: Vec::new(),
            busy_declined: Vec::new(),
            swap_pending: None,
            swap_accept_queued: None,
            privacy_allow_calls: None,
            privacy_p2p: None,
            privacy_loading: false,
            privacy_pending: 0,
            privacy_error: false,
            prefs: CallPrefs::default(),
            recent_calls: Vec::new(),
            recent_calls_offset: String::new(),
            recent_calls_loading: false,
            recent_calls_error: false,
            recent_calls_clearing: false,
            active_group_call: None,
            group_call_fetch_queue: Vec::new(),
            pending_live_story_join: None,
        }
    }
}
