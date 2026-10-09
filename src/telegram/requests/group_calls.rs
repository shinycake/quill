use crate::ids::RequestId;
use serde_json::{Value, json};

/// Phase C3a: a `MessageSender` reference for group-call request
/// fields (e.g. `toggleGroupCallParticipantIsMuted participant_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSenderRef {
    User(i64),
    Chat(i64),
}

impl MessageSenderRef {
    /// Renders `messageSenderUser` (TDLib 1.8.67,
    /// `schema/td_api.tl:2831`) / `messageSenderChat`
    /// (`schema/td_api.tl:2834`), matching the inline convention
    /// used elsewhere in this file.
    pub fn to_value(&self) -> Value {
        match *self {
            MessageSenderRef::User(user_id) => {
                json!({ "@type": "messageSenderUser", "user_id": user_id })
            }
            MessageSenderRef::Chat(chat_id) => {
                json!({ "@type": "messageSenderChat", "chat_id": chat_id })
            }
        }
    }
}

/// Phase C3a: an `InputGroupCall` reference for non-chat-bound group
/// calls (TDLib 1.8.67, `schema/td_api.tl:7242` /
/// `schema/td_api.tl:7247`):
/// `inputGroupCallLink link:string = InputGroupCall;`
/// `inputGroupCallMessage chat_id:int53 message_id:int53 =
/// InputGroupCall;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputGroupCallRef {
    Link(String),
    Message { chat_id: i64, message_id: i64 },
}

impl InputGroupCallRef {
    pub fn to_value(&self) -> Value {
        match self {
            InputGroupCallRef::Link(link) => {
                json!({ "@type": "inputGroupCallLink", "link": link })
            }
            InputGroupCallRef::Message {
                chat_id,
                message_id,
            } => {
                json!({
                    "@type": "inputGroupCallMessage",
                    "chat_id": chat_id,
                    "message_id": message_id,
                })
            }
        }
    }
}

/// Phase C3a: `groupCallJoinParameters` (TDLib 1.8.67,
/// `schema/td_api.tl:7089`):
/// `groupCallJoinParameters audio_source_id:int32 payload:string
/// is_muted:Bool is_my_video_enabled:Bool = GroupCallJoinParameters;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCallJoinParams {
    pub audio_source_id: i32,
    pub payload: String,
    pub is_muted: bool,
    pub is_my_video_enabled: bool,
}

impl GroupCallJoinParams {
    /// The honest no-device join: `audio_source_id` is 0 and `payload`
    /// is empty. Used when no call engine is available or the native
    /// offer fails — TDLib accepts these and the join still goes out;
    /// Phase C2g normally replaces this with the real tgcalls offer.
    pub fn honest_no_device() -> Self {
        GroupCallJoinParams {
            audio_source_id: 0,
            payload: String::new(),
            is_muted: false,
            is_my_video_enabled: false,
        }
    }

    pub fn to_value(&self) -> Value {
        json!({
            "@type": "groupCallJoinParameters",
            "audio_source_id": self.audio_source_id,
            "payload": self.payload,
            "is_muted": self.is_muted,
            "is_my_video_enabled": self.is_my_video_enabled,
        })
    }
}

/// Phase C3a: `createVideoChat` (TDLib 1.8.67,
/// `schema/td_api.tl:14256`):
/// `createVideoChat chat_id:int53 title:string start_date:int32
/// is_rtmp_stream:Bool = GroupCallId;`
/// This is the chat-bound voice/video-chat creation path (groups and
/// channels). An immediate voice chat passes `start_date: 0` and
/// `is_rtmp_stream: false`. (`createGroupCall` is for group calls that
/// *aren't* bound to a chat.)
pub fn create_video_chat(
    extra: RequestId,
    chat_id: i64,
    title: &str,
    start_date: i32,
    is_rtmp_stream: bool,
) -> String {
    json!({
        "@type": "createVideoChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "title": title,
        "start_date": start_date,
        "is_rtmp_stream": is_rtmp_stream,
    })
    .to_string()
}

/// Phase C3a: `createGroupCall` (TDLib 1.8.67,
/// `schema/td_api.tl:14259`):
/// `createGroupCall join_parameters:groupCallJoinParameters =
/// GroupCallInfo;`
/// Creates a group call that isn't bound to a chat. Per the schema
/// docs, pass null `join_parameters` to only create the call link
/// without joining the call.
pub fn create_group_call(extra: RequestId, join_params: Option<&GroupCallJoinParams>) -> String {
    json!({
        "@type": "createGroupCall",
        "@extra": extra.as_extra(),
        "join_parameters": join_params.map(|p| p.to_value()).unwrap_or(Value::Null),
    })
    .to_string()
}

/// Phase C3a: `joinVideoChat` (TDLib 1.8.67,
/// `schema/td_api.tl:14292`):
/// `joinVideoChat group_call_id:int32 participant_id:MessageSender
/// join_parameters:groupCallJoinParameters invite_hash:string = Text;`
/// "Joins an active video chat. Returns join response payload for
/// tgcalls". `participant_id: None` serializes null (join as self).
/// The returned payload is stored by the driver and never consumed —
/// there is no media transport until Phase C2.
pub fn join_video_chat(
    extra: RequestId,
    group_call_id: i32,
    participant_id: Option<&MessageSenderRef>,
    join_params: &GroupCallJoinParams,
    invite_hash: &str,
) -> String {
    json!({
        "@type": "joinVideoChat",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "participant_id": participant_id.map(|p| p.to_value()).unwrap_or(Value::Null),
        "join_parameters": join_params.to_value(),
        "invite_hash": invite_hash,
    })
    .to_string()
}

/// Join an active live story (TDLib `joinLiveStory`); returns the tgcalls payload.
pub fn join_live_story(
    extra: RequestId,
    group_call_id: i32,
    params: &GroupCallJoinParams,
) -> String {
    json!({
        "@type": "joinLiveStory",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "join_parameters": params.to_value(),
    })
    .to_string()
}

/// Phase C3a: `joinGroupCall` (TDLib 1.8.67,
/// `schema/td_api.tl:14285`):
/// `joinGroupCall input_group_call:InputGroupCall
/// join_parameters:groupCallJoinParameters = GroupCallInfo;`
/// Joins a regular group call that is not bound to a chat.
pub fn join_group_call(
    extra: RequestId,
    input_group_call: &InputGroupCallRef,
    join_params: &GroupCallJoinParams,
) -> String {
    json!({
        "@type": "joinGroupCall",
        "@extra": extra.as_extra(),
        "input_group_call": input_group_call.to_value(),
        "join_parameters": join_params.to_value(),
    })
    .to_string()
}

/// Phase C3a: `getGroupCall` (TDLib 1.8.67, `schema/td_api.tl:14274`):
/// `getGroupCall group_call_id:int32 = GroupCall;`
pub fn get_group_call(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "getGroupCall",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C3a: `getGroupCallParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14449`):
/// `getGroupCallParticipants input_group_call:InputGroupCall
/// limit:int32 = GroupCallParticipants;`
/// "Returns information about participants of a non-joined group call
/// that is not bound to a chat".
pub fn get_group_call_participants(
    extra: RequestId,
    input_group_call: &InputGroupCallRef,
    limit: i32,
) -> String {
    json!({
        "@type": "getGroupCallParticipants",
        "@extra": extra.as_extra(),
        "input_group_call": input_group_call.to_value(),
        "limit": limit,
    })
    .to_string()
}

/// Phase C3a: `loadGroupCallParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14455`):
/// `loadGroupCallParticipants group_call_id:int32 limit:int32 = Ok;`
/// "Loads more participants of a group call … The group call must be
/// previously received through getGroupCall and must be joined or
/// being joined". Limit up to 100.
pub fn load_group_call_participants(extra: RequestId, group_call_id: i32, limit: i32) -> String {
    json!({
        "@type": "loadGroupCallParticipants",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "limit": limit,
    })
    .to_string()
}

/// Phase C3a: `leaveGroupCall` (TDLib 1.8.67,
/// `schema/td_api.tl:14458`): `leaveGroupCall group_call_id:int32 =
/// Ok;` "Leaves a group call".
pub fn leave_group_call(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "leaveGroupCall",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C3a: `endGroupCall` (TDLib 1.8.67, `schema/td_api.tl:14461`):
/// `endGroupCall group_call_id:int32 = Ok;` "Ends a group call.
/// Requires groupCall.can_be_managed right for video chats and live
/// stories or groupCall.is_owned otherwise".
pub fn end_group_call(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "endGroupCall",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C2g: `startGroupCallScreenSharing` (TDLib 1.8.67,
/// `schema/td_api.tl:14303`):
/// `startGroupCallScreenSharing group_call_id:int32 audio_source_id:int32
/// payload:string = Text;`
/// "Starts screen sharing in a group call". The `payload` is the
/// presentation offer from `ntg_init_presentation`; the returned `Text`
/// is the answer for `ntg_connect(..., is_presentation=true)`. No
/// separate screen audio source exists in this slice (`audio_source_id`
/// is 0).
pub fn start_group_call_screen_sharing(
    extra: RequestId,
    group_call_id: i32,
    payload: &str,
) -> String {
    json!({
        "@type": "startGroupCallScreenSharing",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "audio_source_id": 0,
        "payload": payload,
    })
    .to_string()
}

/// Phase C2g: `endGroupCallScreenSharing` (TDLib 1.8.67,
/// `schema/td_api.tl:14309`): `endGroupCallScreenSharing
/// group_call_id:int32 = Ok;` "Ends screen sharing in a group call".
pub fn end_group_call_screen_sharing(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "endGroupCallScreenSharing",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C3a: `toggleGroupCallIsMyVideoEnabled` (TDLib 1.8.67,
/// `schema/td_api.tl:14414`):
/// `toggleGroupCallIsMyVideoEnabled group_call_id:int32
/// is_my_video_enabled:Bool = Ok;`
/// Signaling-only in this slice: tracks state, no camera (Phase C2).
pub fn toggle_group_call_is_my_video_enabled(
    extra: RequestId,
    group_call_id: i32,
    is_enabled: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallIsMyVideoEnabled",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "is_my_video_enabled": is_enabled,
    })
    .to_string()
}

/// Phase C3a: `toggleGroupCallIsMyVideoPaused` (TDLib 1.8.67,
/// `schema/td_api.tl:14411`):
/// `toggleGroupCallIsMyVideoPaused group_call_id:int32
/// is_my_video_paused:Bool = Ok;`
/// Signaling-only in this slice: tracks state, no camera (Phase C2).
pub fn toggle_group_call_is_my_video_paused(
    extra: RequestId,
    group_call_id: i32,
    is_paused: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallIsMyVideoPaused",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "is_my_video_paused": is_paused,
    })
    .to_string()
}

/// Phase C3a: `toggleGroupCallParticipantIsMuted` (TDLib 1.8.67,
/// `schema/td_api.tl:14431`):
/// `toggleGroupCallParticipantIsMuted group_call_id:int32
/// participant_id:MessageSender is_muted:Bool = Ok;`
/// "Toggles whether a participant of an active group call is muted,
/// unmuted, or allowed to unmute themselves; not supported for live
/// stories". Gate the UI on the participant's `can_be_muted_for_all_users`
/// / `can_be_unmuted_for_all_users` flags.
pub fn toggle_group_call_participant_is_muted(
    extra: RequestId,
    group_call_id: i32,
    participant_id: &MessageSenderRef,
    is_muted: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallParticipantIsMuted",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "participant_id": participant_id.to_value(),
        "is_muted": is_muted,
    })
    .to_string()
}

/// Phase C3a: `toggleGroupCallParticipantIsHandRaised` (TDLib 1.8.67,
/// `schema/td_api.tl:14444`):
/// `toggleGroupCallParticipantIsHandRaised group_call_id:int32
/// participant_id:MessageSender is_hand_raised:Bool = Ok;`
/// "for video chats only … Only self hand can be raised. Requires
/// groupCall.can_be_managed right to lower other's hand".
pub fn toggle_group_call_participant_is_hand_raised(
    extra: RequestId,
    group_call_id: i32,
    participant_id: &MessageSenderRef,
    is_hand_raised: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallParticipantIsHandRaised",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "participant_id": participant_id.to_value(),
        "is_hand_raised": is_hand_raised,
    })
    .to_string()
}

/// Phase C3a: `toggleVideoChatMuteNewParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14317`):
/// `toggleVideoChatMuteNewParticipants group_call_id:int32
/// mute_new_participants:Bool = Ok;`
/// "Toggles whether new participants of a video chat can be unmuted
/// only by administrators of the video chat. Requires
/// groupCall.can_toggle_mute_new_participants right".
pub fn toggle_video_chat_mute_new_participants(
    extra: RequestId,
    group_call_id: i32,
    mute_new_participants: bool,
) -> String {
    json!({
        "@type": "toggleVideoChatMuteNewParticipants",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "mute_new_participants": mute_new_participants,
    })
    .to_string()
}

/// Phase C3a: `setVideoChatTitle` (TDLib 1.8.67,
/// `schema/td_api.tl:14312`):
/// `setVideoChatTitle group_call_id:int32 title:string = Ok;`
/// "Sets title of a video chat; requires groupCall.can_be_managed
/// right". Title is 1-64 characters.
pub fn set_video_chat_title(extra: RequestId, group_call_id: i32, title: &str) -> String {
    json!({
        "@type": "setVideoChatTitle",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "title": title,
    })
    .to_string()
}

/// Phase C3a: `getVideoChatInviteLink` (TDLib 1.8.67,
/// `schema/td_api.tl:14395`):
/// `getVideoChatInviteLink group_call_id:int32 can_self_unmute:Bool =
/// HttpUrl;`
/// "Returns invite link to a video chat in a public chat".
/// `can_self_unmute: true` requires `groupCall.can_be_managed`.
pub fn get_video_chat_invite_link(
    extra: RequestId,
    group_call_id: i32,
    can_self_unmute: bool,
) -> String {
    json!({
        "@type": "getVideoChatInviteLink",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "can_self_unmute": can_self_unmute,
    })
    .to_string()
}

/// `getVideoChatAvailableParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14638`):
/// `getVideoChatAvailableParticipants chat_id:int53 = MessageSenders;`
/// "Returns list of participant identifiers, on whose behalf a video chat
/// in the chat can be joined".
pub fn get_video_chat_available_participants(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "getVideoChatAvailableParticipants",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// `setVideoChatDefaultParticipant` (TDLib 1.8.67,
/// `schema/td_api.tl:14643`):
/// `setVideoChatDefaultParticipant chat_id:int53
/// default_participant_id:MessageSender = Ok;`
pub fn set_video_chat_default_participant(
    extra: RequestId,
    chat_id: i64,
    default_participant_id: &MessageSenderRef,
) -> String {
    json!({
        "@type": "setVideoChatDefaultParticipant",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "default_participant_id": default_participant_id.to_value(),
    })
    .to_string()
}

/// Phase C2h: `revokeGroupCallInviteLink` (TDLib 1.8.67,
/// `schema/td_api.tl:14398`):
/// `revokeGroupCallInviteLink group_call_id:int32 = Ok;`
/// "Revokes invite link for a group call. Requires
/// groupCall.can_be_managed right for video chats or
/// groupCall.is_owned otherwise".
pub fn revoke_group_call_invite_link(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "revokeGroupCallInviteLink",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C2h: `startGroupCallRecording` (TDLib 1.8.67,
/// `schema/td_api.tl:14405`):
/// `startGroupCallRecording group_call_id:int32 title:string
/// record_video:Bool use_portrait_orientation:Bool = Ok;`
/// "Starts recording of an active group call; for video chats only.
/// Requires groupCall.can_be_managed right". Title is 0-64
/// characters; ongoing state arrives as `groupCall.record_duration`
/// / `is_video_recorded` (schema 1.8.67, lines 7151-7152).
pub fn start_group_call_recording(
    extra: RequestId,
    group_call_id: i32,
    title: &str,
    record_video: bool,
    use_portrait_orientation: bool,
) -> String {
    json!({
        "@type": "startGroupCallRecording",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "title": title,
        "record_video": record_video,
        "use_portrait_orientation": use_portrait_orientation,
    })
    .to_string()
}

/// Phase C2h: `endGroupCallRecording` (TDLib 1.8.67,
/// `schema/td_api.tl:14408`):
/// `endGroupCallRecording group_call_id:int32 = Ok;`
/// "Ends recording of an active group call; for video chats only.
/// Requires groupCall.can_be_managed right".
pub fn end_group_call_recording(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "endGroupCallRecording",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C2h: `startScheduledVideoChat` (TDLib 1.8.67,
/// `schema/td_api.tl:14277`):
/// `startScheduledVideoChat group_call_id:int32 = Ok;`
/// "Starts a scheduled video chat". The schema names no explicit
/// right for this constructor; the driver gates it on
/// `groupCall.can_be_managed` (the tracked proxy for the
/// `can_manage_video_chats` admin right), same as the other
/// video-chat admin actions.
pub fn start_scheduled_video_chat(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "startScheduledVideoChat",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// `toggleVideoChatEnabledStartNotification` (TDLib 1.8.67,
/// `schema/td_api.tl:14282`):
/// `toggleVideoChatEnabledStartNotification group_call_id:int32
/// enabled_start_notification:Bool = Ok;`
/// "Toggles whether the current user will receive a notification
/// when the group call starts; for video chats only". The new flag
/// arrives back as `updateGroupCall` (`groupCall.enabled_start_notification`, :7154).
pub fn toggle_video_chat_enabled_start_notification(
    extra: RequestId,
    group_call_id: i32,
    enabled_start_notification: bool,
) -> String {
    json!({
        "@type": "toggleVideoChatEnabledStartNotification",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "enabled_start_notification": enabled_start_notification,
    })
    .to_string()
}

/// Phase C2h: `getVideoChatRtmpUrl` (TDLib 1.8.67,
/// `schema/td_api.tl:14261`):
/// `getVideoChatRtmpUrl chat_id:int53 = RtmpUrl;`
/// "Returns RTMP URL for streaming to the video chat of a chat;
/// requires can_manage_video_chats administrator right".
pub fn get_video_chat_rtmp_url(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "getVideoChatRtmpUrl",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// Phase C2h: `replaceVideoChatRtmpUrl` (TDLib 1.8.67,
/// `schema/td_api.tl:14264`):
/// `replaceVideoChatRtmpUrl chat_id:int53 = RtmpUrl;`
/// "Replaces the current RTMP URL for streaming to the video chat of
/// a chat; requires owner privileges in the chat".
pub fn replace_video_chat_rtmp_url(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "replaceVideoChatRtmpUrl",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// Phase C2h: `sendGroupCallMessage` (TDLib 1.8.67,
/// `schema/td_api.tl:14341`):
/// `sendGroupCallMessage group_call_id:int32 text:formattedText
/// paid_message_star_count:int53 = Ok;`
/// "Sends a message to other participants of a group call. Requires
/// groupCall.can_send_messages right". Plain text only (empty
/// entities); `paid_message_star_count` is 0 — paid messages are a
/// live-story-only feature Quill doesn't surface.
pub fn send_group_call_message(extra: RequestId, group_call_id: i32, text: &str) -> String {
    json!({
        "@type": "sendGroupCallMessage",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "text": { "@type": "formattedText", "text": text, "entities": [] },
        "paid_message_star_count": 0,
    })
    .to_string()
}

/// Phase C2h: `toggleGroupCallAreMessagesAllowed` (TDLib 1.8.67,
/// `schema/td_api.tl:14322`):
/// `toggleGroupCallAreMessagesAllowed group_call_id:int32
/// are_messages_allowed:Bool = Ok;`
/// "Toggles whether participants of a group call can send messages
/// there. Requires groupCall.can_toggle_are_messages_allowed right".
pub fn toggle_group_call_are_messages_allowed(
    extra: RequestId,
    group_call_id: i32,
    are_messages_allowed: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallAreMessagesAllowed",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "are_messages_allowed": are_messages_allowed,
    })
    .to_string()
}

/// Phase C3a: `declineGroupCallInvitation` (TDLib 1.8.67,
/// `schema/td_api.tl:14380`):
/// `declineGroupCallInvitation chat_id:int53 message_id:int53 = Ok;`
/// Declines a `messageGroupCall` invitation (`schema/td_api.tl:5288`
/// flow: `joinGroupCall` to accept, `declineGroupCallInvitation` to
/// decline).
pub fn decline_group_call_invitation(extra: RequestId, chat_id: i64, message_id: i64) -> String {
    json!({
        "@type": "declineGroupCallInvitation",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "message_id": message_id,
    })
    .to_string()
}

/// Phase C2f: `inviteGroupCallParticipant` (TDLib 1.8.67,
/// `schema/td_api.tl:14375`):
/// `inviteGroupCallParticipant group_call_id:int32 user_id:int53
/// is_video:Bool = InviteGroupCallParticipantResult;`
/// "Invites a user to an active group call". The answer is one of
/// the `inviteGroupCallParticipantResult*` variants (schema 1.8.67,
/// lines 7216-7227), parsed by the envelope.
pub fn invite_group_call_participant(
    extra: RequestId,
    group_call_id: i32,
    user_id: i64,
    is_video: bool,
) -> String {
    json!({
        "@type": "inviteGroupCallParticipant",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "user_id": user_id,
        "is_video": is_video,
    })
    .to_string()
}

/// Phase C2f: `banGroupCallParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14385`):
/// `banGroupCallParticipants group_call_id:int32 user_ids:vector<int64>
/// = Ok;` "Identifiers of group call participants to ban".
pub fn ban_group_call_participants(
    extra: RequestId,
    group_call_id: i32,
    user_ids: &[i64],
) -> String {
    json!({
        "@type": "banGroupCallParticipants",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "user_ids": user_ids,
    })
    .to_string()
}

/// Phase C2f: `setGroupCallParticipantVolumeLevel` (TDLib 1.8.67,
/// `schema/td_api.tl:14438`):
/// `setGroupCallParticipantVolumeLevel group_call_id:int32
/// participant_id:MessageSender volume_level:int32 = Ok;`
/// "New participant's volume level; 1-20000 in hundreds of percents"
/// — the driver clamps before sending.
pub fn set_group_call_participant_volume_level(
    extra: RequestId,
    group_call_id: i32,
    participant_id: &MessageSenderRef,
    volume_level: i32,
) -> String {
    json!({
        "@type": "setGroupCallParticipantVolumeLevel",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "participant_id": participant_id.to_value(),
        "volume_level": volume_level,
    })
    .to_string()
}
