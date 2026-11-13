//! One-to-one calls, group calls, recent calls and call privacy: the `calls` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct CallsState {
    /// The `createCall` asks out: one at a time, until the answer, the
    /// error or the loss of its own request.
    pub creates: crate::state::held::Holds<()>,
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
    /// Calls whose `getGroupCall` failed, by call id. The one who asked
    /// for such a call (a stream wish) takes its entry
    /// ([`CallsState::take_group_call_fetch_failure`]), so a failure ends
    /// the wish of that call and no other.
    pub group_call_fetch_failures: Vec<i32>,
    /// Phase C1: incoming calls that arrived while another call was
    /// active — the driver discards them (busy) via `discardCall`.
    /// Entries are `(call_id, user_id, is_video)` so the decline
    /// reports the actual call kind rather than a hardcoded one.
    pub busy_decline_queue: Vec<(i32, i64, bool)>,
    /// Outgoing calls the server created (`callId`) after another call
    /// took the slot: the driver discards them, `(call_id, is_video)`, so
    /// the callee is not rung to timeout. No "declined" banner: the user
    /// placed it.
    pub created_call_discards: Vec<(i32, bool)>,
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
    /// Phase C2i: local call preferences (confirm-before-calling,
    /// less-data), persisted via `settings::CallPrefs`. The driver
    /// loads them at startup; the UI saves on toggle.
    pub prefs: CallPrefs,
    /// Phase C2i: recent calls from `searchCallMessages` (server-side
    /// history, schema 1.8.67 :11903) for the Recent-calls tab, newest
    /// first.
    pub recent_calls: RecentCalls,
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
    /// `createVideoChat` requests the server answered with the new call,
    /// until the UI lets go of them (matched by request id), so a box that
    /// waits on its create ends the wait on either answer.
    pub answered_creates: Vec<RequestId>,
    /// Phase C3a: group-call ids whose full `groupCall` still needs a
    /// `getGroupCall` fetch (queued from the `createVideoChat`
    /// `groupCallId` answer). Drained by the driver.
    pub group_call_fetch_queue: Vec<i32>,
    /// stories-live-play: the story viewer's "Join live" asked for this
    /// group call; the driver issues `join_video_chat` once the
    /// `getGroupCall` answer has created the unjoined tracker.
    pub pending_live_story_join: Option<LiveStoryJoinIntent>,
    /// A voice chat, video chat or live stream link: the question to leave
    /// a running call, the wait for it to end, what the offer found, then
    /// the invite hash that goes out with the join (`crate::call_link`).
    pub call_link_stage: crate::call_link::CallLinkStage,
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
}

impl CallsState {
    /// A recent-calls page is in flight.
    pub fn recent_calls_loading(&self) -> bool {
        self.recent_calls.loading()
    }

    /// The last `searchCallMessages` request failed.
    pub fn recent_calls_error(&self) -> bool {
        self.recent_calls.failed()
    }

    /// A `deleteAllCallMessages` request is in flight.
    pub fn recent_calls_clearing(&self) -> bool {
        self.recent_calls.clearing()
    }

    /// The tracked group call `id`: the current one or the story stream.
    pub fn group_call(&self, id: i32) -> Option<&ActiveGroupCall> {
        [self.active_group_call.as_ref(), self.story_stream.as_ref()]
            .into_iter()
            .flatten()
            .find(|call| call.id == id)
    }

    /// The invitations "Add People" sent into the current conference.
    pub fn invite_results(&self) -> Option<&ConferenceInviteResults> {
        self.active_group_call.as_ref()?.invite_results.as_ref()
    }

    pub fn invite_results_mut(&mut self) -> Option<&mut ConferenceInviteResults> {
        self.active_group_call.as_mut()?.invite_results.as_mut()
    }

    /// The toast of the current conference's invitations is dismissed.
    pub fn clear_invite_results(&mut self) {
        if let Some(call) = self.active_group_call.as_mut() {
            call.invite_results = None;
        }
    }

    /// The group call `id` was just left (`leaveGroupCall`) and TDLib has
    /// not yet said it is no longer joined.
    pub fn group_call_left(&self, id: i32) -> bool {
        self.group_call(id)
            .is_some_and(|call| call.rejoin.is_left())
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

    /// A `getGroupCall` for `group_call_id` failed.
    pub fn note_group_call_fetch_failed(&mut self, group_call_id: i32) {
        if !self.group_call_fetch_failures.contains(&group_call_id) {
            self.group_call_fetch_failures.push(group_call_id);
        }
    }

    /// Whether the fetch of `group_call_id` failed and the mark still
    /// stands (nothing took it).
    pub fn group_call_fetch_failed(&self, group_call_id: i32) -> bool {
        self.group_call_fetch_failures.contains(&group_call_id)
    }

    /// Whether the fetch of `group_call_id` failed since it was last taken
    /// or asked again; taking it clears the mark.
    pub fn take_group_call_fetch_failure(&mut self, group_call_id: i32) -> bool {
        let before = self.group_call_fetch_failures.len();
        self.group_call_fetch_failures
            .retain(|id| *id != group_call_id);
        self.group_call_fetch_failures.len() != before
    }

    pub(crate) fn new() -> Self {
        Self {
            creates: Default::default(),
            active_call: None,
            summary: None,
            error: None,
            group_call_error: None,
            group_call_fetch_failures: Vec::new(),
            busy_decline_queue: Vec::new(),
            created_call_discards: Vec::new(),
            busy_declined_calls: std::collections::VecDeque::new(),
            busy_declined: Vec::new(),
            second_call: SecondCall::None,
            prefs: CallPrefs::default(),
            recent_calls: RecentCalls::default(),
            active_group_call: None,
            story_stream: None,
            rtmp_start: None,
            create_join_as: None,
            failed_creates: Vec::new(),
            answered_creates: Vec::new(),
            group_call_fetch_queue: Vec::new(),
            pending_live_story_join: None,
            call_link_stage: crate::call_link::CallLinkStage::Idle,
            pending_conference: None,
            call_upgrade: None,
            conference_invite: None,
            conference_invite_declines: Vec::new(),
            add_people_search: ShareSearch::default(),
        }
    }
}
