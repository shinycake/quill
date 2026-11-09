//! Connect driver: message widgets' requests (docs/decisions/message-widgets.md).
//! The answers land in `Session::bots.widget_answers`.
use super::*;
use crate::ids::RequestId;
use crate::message_widget::WidgetKey;
use crate::state::{BotsPurpose, RequestPurpose};
use crate::telegram::requests::{get_widget_public_data, get_widget_query_answer};

/// The most a widget's query carries, in bytes (widget-sandbox checks it
/// first; this keeps a bad runtime from sending more).
const MAX_QUERY_BYTES: usize = 64;

impl<S: JsonSender> ConnectDriver<S> {
    /// A widget message that can make requests: signed in, and a sent
    /// message (a pending or scheduled one has no server id yet).
    fn widget_request_allowed(&self, key: WidgetKey) -> bool {
        self.chats_path_active() && key.message_id.0 > 0 && key.chat_id.0 != 0
    }

    /// The widget's request `token` to its bot (`getWidgetQueryAnswer`).
    pub fn send_widget_query(
        &mut self,
        key: WidgetKey,
        token: u64,
        data: &[u8],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.widget_request_allowed(key) || data.is_empty() || data.len() > MAX_QUERY_BYTES {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Bots(BotsPurpose::GetWidgetQueryAnswer { token });
        let extra = self
            .session
            .request_for_message(purpose, key.chat_id, key.message_id);
        let json = get_widget_query_answer(extra, key.chat_id, key.message_id, data);
        self.send_json_request(extra, &json)
    }

    /// Poll an on-screen widget's public data (`getWidgetPublicData`); one
    /// poll per widget at a time.
    pub fn poll_widget_public_data(
        &mut self,
        key: WidgetKey,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.widget_request_allowed(key) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Bots(BotsPurpose::GetWidgetPublicData);
        if self
            .session
            .requests
            .has_purpose_for_message(purpose, key.chat_id, key.message_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_message(purpose, key.chat_id, key.message_id);
        let json = get_widget_public_data(extra, key.chat_id, key.message_id);
        self.send_json_request(extra, &json).map(Some)
    }

    /// The widget answers TDLib gave since the last call, oldest first.
    pub fn take_widget_answers(&mut self) -> Vec<crate::message_widget::WidgetAnswer> {
        std::mem::take(&mut self.session.bots.widget_answers)
    }
}
