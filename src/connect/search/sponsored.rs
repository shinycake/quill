//! Connect driver: sponsored messages and chats.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// `getChatSponsoredMessages` for a channel chat (TDLib 1.8.67). Called
    /// when a channel is opened; rows render Sponsored / Recommended.
    /// The fetch already runs so the pipeline is proven with replay
    /// fixtures. Bot chats can also carry sponsored messages per the
    /// schema; they are not fetched yet (Phase 3).
    pub fn fetch_sponsored_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_channel = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
        if !is_channel {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatSponsoredMessages, chat_id)
            || !self
                .session
                .sponsored_fetch_due(chat_id, std::time::Instant::now())
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatSponsoredMessages, Some(chat_id));
        match self
            .sender
            .send_json(&get_chat_sponsored_messages(extra, chat_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Tell TDLib the sponsored messages `shown` are on screen
    /// (`viewMessages`, `messageSourceChatHistory`; TDLib 1.8.67 has no
    /// separate `viewSponsoredMessage`). The session counts each id once, so
    /// calling this every frame the ad is visible sends at most one request
    /// per ad. Returns the request id when something was sent.
    pub fn view_sponsored_messages(
        &mut self,
        chat_id: ChatId,
        shown: &[i64],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let due = self.session.take_sponsored_views(chat_id, shown);
        if due.is_empty() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewSponsoredMessages, Some(chat_id));
        let ids: Vec<MessageId> = due.iter().copied().map(MessageId).collect();
        match self.sender.send_json(&view_messages(
            extra,
            chat_id,
            &ids,
            "messageSourceChatHistory",
            false,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.untake_sponsored_views(chat_id, &due);
                Err(err)
            }
        }
    }

    /// "Hide ads" (tdesktop `HideSponsoredClickHandler`): Premium accounts
    /// send `toggleHasSponsoredMessagesEnabled(false)`; for anyone else the
    /// session records the "needs Premium" notice and nothing is sent
    /// (`Ok(None)`).
    pub fn hide_sponsored_messages(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.begin_sponsored_hide(chat_id, message_id) != Some(true) {
            return Ok(None);
        }
        let extra = self.session.request(
            RequestPurpose::ToggleHasSponsoredMessagesEnabled,
            Some(chat_id),
        );
        match self
            .sender
            .send_json(&toggle_has_sponsored_messages_enabled(extra, false))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `reportChatSponsoredMessage` (TDLib 1.8.67). Empty `option_id` starts
    /// the flow; TDLib may answer `reportSponsoredResultOptionRequired`.
    /// Returns `None` when the row is missing or `can_be_reported` is false.
    pub fn report_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        option_id: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .begin_sponsored_report(chat_id, message_id)
            .is_none()
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
        match self.sender.send_json(&report_chat_sponsored_message(
            extra, chat_id, message_id, option_id,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.dismiss_sponsored_report();
                Err(err)
            }
        }
    }

    /// `clickChatSponsoredMessage` (TDLib 1.8.67): the user opened a sponsored
    /// message's sponsor link/button (`is_media_click = false`) or its media
    /// (`is_media_click = true`). Fire-and-forget; the `ok` response is ignored.
    pub fn click_chat_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        is_media_click: bool,
        from_fullscreen: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ClickChatSponsoredMessage, Some(chat_id));
        match self.sender.send_json(&click_chat_sponsored_message(
            extra,
            chat_id,
            message_id,
            is_media_click,
            from_fullscreen,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `viewSponsoredChat` (TDLib 1.8.67): the user fully viewed a sponsored
    /// chat. The unique id comes from `sponsoredChat` search results.
    pub fn view_sponsored_chat(
        &mut self,
        sponsored_chat_unique_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewSponsoredChat, None);
        match self
            .sender
            .send_json(&view_sponsored_chat(extra, sponsored_chat_unique_id))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
