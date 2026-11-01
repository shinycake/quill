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
    /// The incoming call `acceptCall` went out for. Its state stays
    /// `Pending` until TDLib's `updateCall`, but it no longer rings: a
    /// close hangs it up.
    pub answered_call: Option<i32>,
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
    /// The live story the viewer plays: a receive-only group call of its
    /// own beside `active_call` and `active_group_call` (tdesktop
    /// `Calls::Instance::registerVideoStream`). Never the current call:
    /// it sends no media, has no mute or call bar, and goes when the
    /// viewer leaves the story.
    pub story_stream: Option<ActiveGroupCall>,
    /// "Stream with...": the process for one chat (`calls::rtmp_start`).
    pub rtmp_start: Option<crate::calls::rtmp_start::RtmpStart>,
    /// Starting a video chat: the "join as" list asked for first.
    pub create_join_as: Option<crate::calls::group_call_create::CreateJoinAs>,
    /// `createVideoChat` requests the server refused, until the UI shows
    /// them (matched by request id).
    pub failed_creates: Vec<RequestId>,
    /// Phase C3a: group-call ids whose full `groupCall` still needs a
    /// `getGroupCall` fetch (queued from the `createVideoChat`
    /// `groupCallId` answer). Drained by the driver.
    pub group_call_fetch_queue: Vec<i32>,
    /// stories-live-play: the story viewer's "Join live" asked for this
    /// group call; the driver issues `join_video_chat` once the
    /// `getGroupCall` answer has created the unjoined tracker.
    pub pending_live_story_join: Option<LiveStoryJoinIntent>,
    /// A voice chat, video chat or live stream link offered this call;
    /// its invite hash goes out with the join (`crate::call_link`).
    pub call_link: Option<crate::call_link::PendingCallLink>,
    /// A call link arrived while a call is on: the question to leave it.
    pub call_link_confirm: Option<crate::call_link::CallLinkConfirm>,
    /// The confirmed link `(chat id, hash)`, offered once the old call has
    /// ended.
    pub call_link_after_leave: Option<(i64, String)>,
    /// The group call just left with `leaveGroupCall`, until TDLib says
    /// it is no longer joined: a late joined update for it must not take
    /// the slot back.
    pub left_group_call: Option<i32>,
    /// A conference call being joined or created, until its answer names
    /// the call (`crate::state::conference`).
    pub pending_conference: Option<PendingConference>,
    /// The 1:1 call was taken into a conference call by the other side;
    /// the driver joins it (`crate::state::conference`).
    pub call_upgrade: Option<CallUpgrade>,
    /// A conference call invitation ringing now
    /// (`crate::state::conference_invite`).
    pub conference_invite: Option<ConferenceInvite>,
    /// Invitations to decline at once, never shown: they came in while
    /// you were in a call, or are older than the ring timeout (tdesktop
    /// `declineIncomingConferenceInvites`). The driver drains them.
    pub conference_invite_declines: Vec<(ChatId, MessageId)>,
    /// The "Add People" box's server search: `local` holds the
    /// `searchChatsOnServer` answer, `server` the `searchPublicChats` one
    /// (`crate::state::conference_add_people`).
    pub add_people_search: ShareSearch,
    /// The invitations "Add People" sent into the new conference, until
    /// their toast is dismissed (`crate::state::conference_add_people`).
    pub invite_results: Option<ConferenceInviteResults>,
}

impl CallsState {
    /// The tracked group call `id`: the current one or the story stream.
    pub fn group_call(&self, id: i32) -> Option<&ActiveGroupCall> {
        [self.active_group_call.as_ref(), self.story_stream.as_ref()]
            .into_iter()
            .flatten()
            .find(|call| call.id == id)
    }

    pub fn group_call_mut(&mut self, id: i32) -> Option<&mut ActiveGroupCall> {
        [self.active_group_call.as_mut(), self.story_stream.as_mut()]
            .into_iter()
            .flatten()
            .find(|call| call.id == id)
    }

    /// The ids of both slots, current call first.
    pub fn group_call_ids(&self) -> [Option<i32>; 2] {
        [
            self.active_group_call.as_ref().map(|call| call.id),
            self.story_stream.as_ref().map(|call| call.id),
        ]
    }

    /// Whether the id is the current group call, whose errors the call
    /// window shows.
    pub fn is_current_group_call(&self, group_call_id: i32) -> bool {
        self.active_group_call
            .as_ref()
            .is_some_and(|call| call.id == group_call_id)
    }

    pub(crate) fn new() -> Self {
        Self {
            active_call: None,
            summary: None,
            error: None,
            group_call_error: None,
            busy_decline_queue: Vec::new(),
            busy_declined: Vec::new(),
            swap_pending: None,
            answered_call: None,
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
            story_stream: None,
            rtmp_start: None,
            create_join_as: None,
            failed_creates: Vec::new(),
            group_call_fetch_queue: Vec::new(),
            pending_live_story_join: None,
            call_link: None,
            call_link_confirm: None,
            call_link_after_leave: None,
            left_group_call: None,
            pending_conference: None,
            call_upgrade: None,
            conference_invite: None,
            conference_invite_declines: Vec::new(),
            add_people_search: ShareSearch::default(),
            invite_results: None,
        }
    }
}
