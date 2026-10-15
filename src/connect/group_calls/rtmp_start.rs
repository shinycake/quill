//! Connect driver: "Stream with..." for a chat that has no video chat yet
//! (tdesktop `StartRtmpProcess`, calls/group/calls_group_rtmp.cpp).
//!
//! `getVideoChatRtmpUrl` and `replaceVideoChatRtmpUrl` are chat-bound, so
//! they work before any call exists; the answers land in
//! `calls.rtmp_start`. "Start Streaming" is `createVideoChat` with
//! `is_rtmp_stream = true`.
use super::*;
use crate::calls::rtmp_start::RtmpStart;
use crate::ids::ChatId;

impl<S: JsonSender> ConnectDriver<S> {
    /// Open the process for a chat and ask for its RTMP URL. tdesktop
    /// ignores a second start for the same chat (it only refreshes the
    /// callbacks) and drops the first when another chat asks.
    pub fn begin_rtmp_start(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || !self.rtmp_start_allowed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .calls
            .rtmp_start
            .as_ref()
            .is_some_and(|start| start.chat_id == chat_id)
        {
            return Ok(());
        }
        self.session.calls.rtmp_start = Some(RtmpStart::new(chat_id));
        self.send_rtmp_url_request(chat_id, false).inspect_err(|_| {
            self.session.calls.rtmp_start = None;
        })
    }

    /// "Revoke Stream Key" in the box: the same request with `revoke`.
    pub fn revoke_rtmp_start_key(&mut self) -> Result<(), ConnectSendError> {
        let Some(chat_id) = self.session.calls.rtmp_start.as_ref().map(|s| s.chat_id) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_rtmp_url_request(chat_id, true)
    }

    /// "Start Streaming": create the video chat as an RTMP stream with the
    /// chat's title (tdesktop passes none, TDLib then uses the chat title)
    /// and end the process.
    pub fn start_rtmp_stream(&mut self) -> Result<RequestId, ConnectSendError> {
        let Some(chat_id) = self.session.calls.rtmp_start.as_ref().map(|s| s.chat_id) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let sent = self.create_video_chat_request(chat_id.0, String::new(), 0, true)?;
        self.session.calls.rtmp_start = None;
        Ok(sent)
    }

    /// Cancel / close the box (`StartRtmpProcess::close`).
    pub fn close_rtmp_start(&mut self) {
        self.session.calls.rtmp_start = None;
    }

    /// A group or channel the viewer can manage video chats in, without a
    /// video chat yet.
    fn rtmp_start_allowed(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return false;
        };
        matches!(
            chat.kind,
            ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. }
        ) && chat.video_chat.is_none()
            && self.session.chat_can_manage_video_chats(chat_id)
    }

    fn send_rtmp_url_request(
        &mut self,
        chat_id: ChatId,
        revoke: bool,
    ) -> Result<(), ConnectSendError> {
        let purpose = if revoke {
            CallsPurpose::ReplaceVideoChatRtmpUrl { chat_id: chat_id.0 }
        } else {
            CallsPurpose::GetVideoChatRtmpUrl { chat_id: chat_id.0 }
        };
        let extra = self.session.request(RequestPurpose::Calls(purpose), None);
        let json = if revoke {
            replace_video_chat_rtmp_url(extra, chat_id.0)
        } else {
            get_video_chat_rtmp_url(extra, chat_id.0)
        };
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(start) = self.session.calls.rtmp_start.as_mut() {
            start.sent(extra);
        }
        Ok(())
    }
}
