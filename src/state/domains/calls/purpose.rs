//! Request purposes for one-to-one calls, group calls and video chats.
use crate::state::request_purpose::flat_purposes;
use crate::state::*;

/// In-flight requests for one-to-one calls, group calls and video chats; wrapped as
/// [`RequestPurpose::Calls`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallsPurpose {
    /// Phase C1: `createCall`. Response is `callId`; correlated via
    /// `PendingRequest::user_id`. The call's states arrive as
    /// `updateCall`. Phase C1b: `is_video` rides along so the `callId`
    /// answer can start tracking with the right call kind — the answer
    /// itself carries no `is_video` (schema 1.8.67, :7034).
    CreateCall { is_video: bool },
    /// Phase C1: `acceptCall`. Response is `ok`; the answered state
    /// arrives as `updateCall`.
    AcceptCall,
    /// Phase C2b: `sendCallSignalingData`. Response is `ok`; this is a
    /// fire-and-forget bridge from the call engine to TDLib.
    SendCallSignalingData,
    /// Phase C1: `discardCall`. Response is `ok`; the hangup states
    /// (`callStateHangingUp` → `callStateDiscarded`) arrive as
    /// `updateCall`.
    DiscardCall,
    /// Phase C1: `sendCallRating`. Response is `ok`; sent from the
    /// call-end rating card when `callStateDiscarded.need_rating`.
    SendCallRating,
    /// Phase C2d: `sendCallDebugInformation` for the ended call.
    SendCallDebugInformation,
    /// Phase C2i: `searchCallMessages`. Response is `foundMessages`;
    /// drives the Recent-calls tab.
    SearchCallMessages,
    /// `deleteAllCallMessages` ("Clear all" on the Calls list). The
    /// answer is `ok`; the cached list is emptied then.
    DeleteAllCallMessages,
    /// Phase C2i: `getUserPrivacySettingRules`. Response is
    /// `userPrivacySettingRules`; `setting` selects which of the two
    /// call privacy settings is fetched.
    GetCallPrivacyRules { setting: CallPrivacySetting },
    /// Phase C2i: `setUserPrivacySettingRules`. Response is `ok`; the
    /// new value is applied optimistically at send time.
    SetCallPrivacyRules { setting: CallPrivacySetting },
    /// Phase C2i: `sendCallLog` for the ended call. Response is `ok`.
    SendCallLog,
    /// Phase C3a: `createVideoChat`. Response is `groupCallId`; the
    /// chat-bound voice chat's states arrive as `updateGroupCall`.
    CreateVideoChat { chat_id: i64 },
    /// Phase C3a: `joinVideoChat` or `joinLiveStory`. Response is `text` (join payload
    /// for tgcalls) — stored on the tracked call; Phase C2g consumes it
    /// in the driver pump to finish the native group handshake.
    JoinVideoChat { group_call_id: i32 },
    /// Phase C3a: `leaveGroupCall`. Response is `ok`.
    LeaveGroupCall { group_call_id: i32 },
    /// Phase C3a: `endGroupCall`. Response is `ok`.
    EndGroupCall { group_call_id: i32 },
    /// Phase C2g: `startGroupCallScreenSharing` (schema 1.8.67, :14303).
    /// Response is `text` — the presentation answer for
    /// `CallEngine::connect_screen_share`.
    StartGroupCallScreenSharing { group_call_id: i32 },
    /// Phase C2g: `endGroupCallScreenSharing` (schema 1.8.67, :14309).
    /// Response is `ok`.
    EndGroupCallScreenSharing { group_call_id: i32 },
    /// Phase C3a: `getGroupCall`. Response is `groupCall`; refreshes
    /// the tracked call via `updateGroupCall`-equivalent handling.
    GetGroupCall { group_call_id: i32 },
    /// Phase C3a: `loadGroupCallParticipants`. Response is `ok`;
    /// participants arrive as updates.
    LoadGroupCallParticipants { group_call_id: i32 },
    /// `getVideoChatAvailableParticipants`. Response is `messageSenders`;
    /// the "join as" choices of the tracked call.
    GetVideoChatAvailableParticipants { group_call_id: i32 },
    /// `getVideoChatAvailableParticipants` before a video chat is created
    /// (`calls.create_join_as`). Response is `messageSenders`.
    CreateVideoChatJoinAs { chat_id: i64 },
    /// `setVideoChatDefaultParticipant`. Response is `ok`.
    SetVideoChatDefaultParticipant { group_call_id: i32 },
    /// `getVideoChatInviteLink`. Response is `httpUrl`: the speaker link
    /// when `can_self_unmute`, otherwise the listener link.
    GetVideoChatInviteLink {
        group_call_id: i32,
        can_self_unmute: bool,
    },
    /// Phase C3a: `setVideoChatTitle`. Response is `ok`; the new title
    /// arrives as `updateGroupCall`.
    SetVideoChatTitle { group_call_id: i32 },
    /// Phase C2h: `revokeGroupCallInviteLink`. Response is `ok`;
    /// clears the cached invite link.
    RevokeVideoChatInviteLink { group_call_id: i32 },
    /// Phase C2h: `startGroupCallRecording`. Response is `ok`;
    /// recording state arrives as `updateGroupCall`
    /// (`record_duration` / `is_video_recorded`).
    StartGroupCallRecording { group_call_id: i32 },
    /// Phase C2h: `endGroupCallRecording`. Response is `ok`;
    /// recording state arrives as `updateGroupCall`.
    EndGroupCallRecording { group_call_id: i32 },
    /// Phase C2h: `startScheduledVideoChat`. Response is `ok`;
    /// the call goes live via `updateGroupCall` /
    /// `updateNewVideoChat`.
    StartScheduledVideoChat { group_call_id: i32 },
    /// `toggleVideoChatEnabledStartNotification` (schema 1.8.67,
    /// :14282). Response is `ok`; the new
    /// `groupCall.enabled_start_notification` arrives as
    /// `updateGroupCall`.
    ToggleVideoChatEnabledStartNotification { group_call_id: i32, enabled: bool },
    /// Phase C2h: `getVideoChatRtmpUrl`. Response is `rtmpUrl`.
    GetVideoChatRtmpUrl { chat_id: i64 },
    /// Phase C2h: `replaceVideoChatRtmpUrl`. Response is `rtmpUrl`.
    ReplaceVideoChatRtmpUrl { chat_id: i64 },
    /// Phase C2h: `sendGroupCallMessage`. Response is `ok`; the
    /// message arrives back as `updateNewGroupCallMessage` (echo),
    /// or `updateGroupCallMessageSendFailed` on failure.
    SendGroupCallMessage { group_call_id: i32 },
    /// Phase C2h: `toggleGroupCallAreMessagesAllowed`. Response is
    /// `ok`; the new flag arrives as `updateGroupCall`.
    ToggleGroupCallAreMessagesAllowed { group_call_id: i32 },
    /// Phase C3a: `toggleGroupCallIsMyVideoEnabled` /
    /// `toggleGroupCallIsMyVideoPaused`, and
    /// `toggleGroupCallScreenSharingIsPaused`. Response is `ok`; state
    /// refreshes via `updateGroupCall` and participant updates.
    ToggleGroupCallVideo { group_call_id: i32 },
    /// Phase C3a: `toggleGroupCallParticipantIsMuted`. Response is
    /// `ok`; state refreshes via updates.
    ToggleGroupCallParticipantMute { group_call_id: i32 },
    /// Phase C3a: `toggleGroupCallParticipantIsHandRaised`. Response is
    /// `ok`; state refreshes via updates.
    ToggleGroupCallParticipantHand { group_call_id: i32 },
    /// Phase C3a: `toggleVideoChatMuteNewParticipants`. Response is
    /// `ok`; state refreshes via `updateGroupCall`.
    ToggleVideoChatMuteNew { group_call_id: i32 },
    /// Phase C2f: `inviteGroupCallParticipant`. Response is
    /// `inviteGroupCallParticipantResult*`; non-success results
    /// surface via `group_call_error`.
    InviteGroupCallParticipant { group_call_id: i32 },
    /// `inviteVideoChatParticipants`, the invite for a call bound to a
    /// chat. Response is `ok`.
    InviteVideoChatParticipants { group_call_id: i32 },
    /// Phase C2f: `banGroupCallParticipants`. Response is `ok`;
    /// the roster refreshes via participant updates.
    BanGroupCallParticipants { group_call_id: i32 },
    /// Phase C2f: `setGroupCallParticipantVolumeLevel`. Response is
    /// `ok`; the new level arrives via `updateGroupCallParticipant`.
    SetGroupCallParticipantVolumeLevel { group_call_id: i32 },
    /// `setGroupCallParticipantIsSpeaking` from the level tap. Response
    /// is a `MessageSender`; your `is_speaking` arrives via
    /// `updateGroupCallParticipant`. Failures are ignored: a missed
    /// speaking mark is not worth a banner.
    SetGroupCallParticipantIsSpeaking { group_call_id: i32 },
    /// `joinGroupCall` or `createGroupCall` for a conference call (an
    /// invitation, a link, a rejoin, or a 1:1 call taken into a new one).
    /// The `groupCallInfo` answer names the call and carries the tgcalls
    /// answer (`Session::accept_conference_joined`).
    JoinConference,
    /// Phase C2f: `declineGroupCallInvitation`. Response is `ok`.
    DeclineGroupCallInvitation { chat_id: i64, message_id: i64 },
    /// Stream mode: `getGroupCallStreams` or `getGroupCallStreamSegment`
    /// for the stream feeder. The driver takes the answer (`groupCallStreams`,
    /// `data` or an error) before the reducer sees it.
    GroupCallStream { group_call_id: i32 },
    /// "Add People" search: `searchChatsOnServer` for the typed query
    /// (tdesktop's `contacts.search` `my_results`). Response is `chats`;
    /// stored in `CallsState::add_people_search`.
    SearchAddPeopleOnServer,
    /// "Add People" search: `searchPublicChats` (`contacts.search`
    /// `results`, the people outside your chats). Response is `chats`.
    SearchAddPeoplePublic,
}

flat_purposes!(Calls(CallsPurpose) {
    AcceptCall,
    SendCallSignalingData,
    DiscardCall,
    SendCallRating,
    SendCallDebugInformation,
    SearchCallMessages,
    DeleteAllCallMessages,
    SendCallLog,
    JoinConference,
    SearchAddPeopleOnServer,
    SearchAddPeoplePublic,
});
