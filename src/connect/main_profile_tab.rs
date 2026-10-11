//! Connect driver: choose the tab your profile opens on.
use super::*;
use crate::ids::RequestId;
use crate::profile_tab::ProfileTab;
use crate::state::RequestPurpose;
use crate::telegram::requests::set_main_profile_tab;

impl<S: JsonSender> ConnectDriver<S> {
    /// `setMainProfileTab` (schema 1.8.68, line 15238). The new value
    /// arrives via `updateUserFullInfo`.
    pub fn set_main_profile_tab(&mut self, tab: ProfileTab) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetMainProfileTab, None);
        self.send_json_request(extra, &set_main_profile_tab(extra, tab))
    }
}
