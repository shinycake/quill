//! TDLib updates and answers for one-to-one calls, group calls and video chats.
mod parse;

use crate::telegram::envelope::*;
pub(crate) use parse::parse_calls_payload;

/// Payloads for one-to-one calls, group calls and video chats; wrapped as
/// [`EnvelopePayload::Calls`].
#[derive(Debug, Clone, PartialEq)]
pub enum CallsPayload {
    /// Phase C1: `updateCall` (schema 1.8.67, line 10816) — a new call
    /// was created or information about a call was updated.
    UpdateCall { call: ParsedCall },
    /// Phase C1: `updateNewCallSignalingData` (schema 1.8.67, line
    /// 10862) — new call signaling data arrived. Quill has no media
    /// transport yet (C2), so the session queues it honestly; nothing
    /// consumes it.
    UpdateNewCallSignalingData { call_id: i32, data: Vec<u8> },
    /// Phase C1: `callId` (schema 1.8.67, line 7034) — the `createCall`
    /// answer. Correlated to the outgoing request via `@extra` /
    /// `RequestPurpose::CreateCall`.
    CallId { id: i32 },
    /// Phase C3a: `groupCallId` (schema 1.8.67, line 7037) — the
    /// `createVideoChat` answer. Correlated via `@extra` /
    /// `RequestPurpose::CreateVideoChat`.
    GroupCallId { id: i32 },
    /// Phase C2f: `groupCallInfo` (schema 1.8.67, line 7190) — the
    /// `joinGroupCall` answer (invitation acceptance). Correlated via
    /// `@extra` / `RequestPurpose::JoinGroupCallInvitation`. The
    /// `join_payload` is the tgcalls payload (stored, never consumed —
    /// no media transport until Phase C2); `updateGroupCall` remains
    /// the source of truth for join state.
    GroupCallInfo {
        group_call_id: i32,
        join_payload: String,
    },
    /// Phase C3a: `updateGroupCall` (schema 1.8.67, line 10819) — a
    /// group call was created or its information was updated.
    UpdateGroupCall { group_call: ParsedGroupCall },
    /// Phase C3a: `updateGroupCallParticipant` (schema 1.8.67, line
    /// 10824) — information about a group call participant changed.
    UpdateGroupCallParticipant {
        group_call_id: i32,
        participant: ParsedGroupCallParticipant,
    },
    /// Phase C3a: `updateGroupCallParticipants` (schema 1.8.67, line
    /// 10830) — the participant list changed; carries only user ids.
    UpdateGroupCallParticipants {
        group_call_id: i32,
        participant_user_ids: Vec<i64>,
    },
    /// Phase C3a: `updateGroupCallVerificationState` (schema 1.8.67,
    /// line 10836) — E2E verification emojis for the group call.
    UpdateGroupCallVerificationState {
        group_call_id: i32,
        generation: i32,
        emojis: Vec<String>,
    },
    /// Phase C3a: `updateChatVideoChat` (schema 1.8.67, line 10576) —
    /// a chat's video chat changed.
    UpdateChatVideoChat {
        chat_id: i64,
        video_chat: ParsedVideoChat,
    },
    /// Phase C2h: `updateNewGroupCallMessage` (schema 1.8.67, line
    /// 10839) — a message was sent in a group call (including by the
    /// current user; the echo is the confirmation).
    UpdateNewGroupCallMessage {
        group_call_id: i32,
        message: ParsedGroupCallMessage,
    },
    /// Phase C2h: `updateGroupCallMessageSendFailed` (schema 1.8.67,
    /// line 10851) — a sent group-call message failed.
    UpdateGroupCallMessageSendFailed {
        group_call_id: i32,
        message_id: i32,
        error: TdError,
    },
    /// Phase C2h: `updateGroupCallMessagesDeleted` (schema 1.8.67,
    /// line 10856) — group-call messages were deleted.
    UpdateGroupCallMessagesDeleted {
        group_call_id: i32,
        message_ids: Vec<i32>,
    },
    /// Phase C2h: `rtmpUrl` (schema 1.8.67, line 7113) — the
    /// `getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl` answer.
    RtmpUrl { url: String, stream_key: String },
    /// Phase C2f: `inviteGroupCallParticipantResult*` (schema 1.8.67,
    /// lines 7216-7227) — the `inviteGroupCallParticipant` answer.
    InviteGroupCallParticipantResult(InviteGroupCallParticipantResult),
}
