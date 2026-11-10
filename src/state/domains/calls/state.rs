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
    /// Ids of the calls declined as busy (newest last, at most 16).
    /// Their later `updateCall`s, the terminal one included, belong to no
    /// call window; the terminal one drops the id.
    pub busy_declined_calls: std::collections::VecDeque<i32>,
    /// Phase C2i: incoming calls auto-declined while busy, kept as
    /// `(user_id, is_video)` so the UI can say so honestly instead of
    /// declining silently. Drained by the UI banner.
    pub busy_declined: Vec<(i64, bool)>,
    /// The first incoming call that arrived while another call was
    /// active: its swap prompt, or the accept queued behind the current
    /// call's hang-up. Further incoming calls go to `busy_decline_queue`.
    pub second_call: SecondCall,
    /// The active call's `discardCall` in flight, which owns its
    /// `HangingUp` phase.
    pub hang_up: Option<HangUp>,
    /// The incoming call `acceptCall` went out for. Its state stays
    /// `Pending` until TDLib's `updateCall`, but it no longer rings: a
    /// close hangs it up.
    pub answered_call: Option<i32>,
    /// Phase C2i: "who can call me"
    /// (`userPrivacySettingAllowCalls`, schema 1.8.67 :9006).
    pub privacy_allow_calls: Option<PrivacyWho>,
    /// Phase C2i: peer-to-peer calls
    /// (`userPrivacySettingAllowPeerToPeerCalls`, schema 1.8.67 :9009).
    pub privacy_p2p: Option<PrivacyWho>,
    /// Where each call-privacy rule's last get or set stands. The fetch
    /// sends two gets, so loading (see [`CallsState::privacy_loading`])
    /// ends only when both answered, and a failed send or answer marks
    /// only its own rule.
    pub privacy_asks: CallPrivacyAsks,
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
    /// The `searchCallMessages` page the list waits for. Only its answer
    /// or error lands: a reopen asks for the first page again at once, and
    /// "Clear all" forgets the page in flight, so an older page never
    /// fills the list.
    pub recent_calls_request: Option<RequestId>,
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
    /// A privacy get/set round-trip is in flight.
    pub fn privacy_loading(&self) -> bool {
        self.privacy_asks.loading()
    }

    /// The last privacy get/set of some rule failed.
    pub fn privacy_error(&self) -> bool {
        self.privacy_asks.failed()
    }

    /// A recent-calls page is in flight.
    pub fn recent_calls_loading(&self) -> bool {
        self.recent_calls_request.is_some()
    }

    /// Whether `request` is the recent-calls page the list waits for; if
    /// so the list stops waiting for it.
    pub(crate) fn take_recent_calls_request(&mut self, request: RequestId) -> bool {
        if self.recent_calls_request != Some(request) {
            return false;
        }
        self.recent_calls_request = None;
        true
    }

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
            busy_declined_calls: std::collections::VecDeque::new(),
            busy_declined: Vec::new(),
            second_call: SecondCall::None,
            hang_up: None,
            answered_call: None,
            privacy_allow_calls: None,
            privacy_p2p: None,
            privacy_asks: CallPrivacyAsks::default(),
            prefs: CallPrefs::default(),
            recent_calls: Vec::new(),
            recent_calls_offset: String::new(),
            recent_calls_request: None,
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

/// Where one call-privacy rule's last request stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrivacyAsk {
    #[default]
    Idle,
    /// Asked as this request; only its answer settles the rule.
    Asking(RequestId),
    Failed,
}

/// The two rules the calls UI reads (`userPrivacySettingAllowCalls` and
/// `userPrivacySettingAllowPeerToPeerCalls`), each with its own stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallPrivacyAsks {
    allow_calls: PrivacyAsk,
    peer_to_peer: PrivacyAsk,
}

impl CallPrivacyAsks {
    fn slot(&mut self, setting: CallPrivacySetting) -> &mut PrivacyAsk {
        match setting {
            CallPrivacySetting::AllowCalls => &mut self.allow_calls,
            CallPrivacySetting::PeerToPeer => &mut self.peer_to_peer,
        }
    }

    pub fn of(self, setting: CallPrivacySetting) -> PrivacyAsk {
        match setting {
            CallPrivacySetting::AllowCalls => self.allow_calls,
            CallPrivacySetting::PeerToPeer => self.peer_to_peer,
        }
    }

    pub fn set(&mut self, setting: CallPrivacySetting, ask: PrivacyAsk) {
        *self.slot(setting) = ask;
    }

    /// `request` answered for `setting`: whether it is the one the rule
    /// waits for (a stale answer changes nothing).
    pub fn answered(&mut self, setting: CallPrivacySetting, request: RequestId) -> bool {
        let slot = self.slot(setting);
        let waits = *slot == PrivacyAsk::Asking(request);
        if waits {
            *slot = PrivacyAsk::Idle;
        }
        waits
    }

    /// `request` failed for `setting`: whether it is the one the rule
    /// waits for.
    pub fn failed_by(&mut self, setting: CallPrivacySetting, request: RequestId) -> bool {
        let slot = self.slot(setting);
        let waits = *slot == PrivacyAsk::Asking(request);
        if waits {
            *slot = PrivacyAsk::Failed;
        }
        waits
    }

    pub fn loading(self) -> bool {
        [self.allow_calls, self.peer_to_peer]
            .iter()
            .any(|ask| matches!(ask, PrivacyAsk::Asking(_)))
    }

    pub fn failed(self) -> bool {
        [self.allow_calls, self.peer_to_peer].contains(&PrivacyAsk::Failed)
    }
}
