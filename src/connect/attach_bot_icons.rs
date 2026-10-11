//! Download the icons of the attachment menu bots (tdesktop saves each
//! bot's `default_static` icon when it parses the list).
use super::*;
use crate::ids::RequestId;

impl<S: JsonSender> ConnectDriver<S> {
    /// Runs on every ingest; `should_download` keeps each icon to one
    /// request until it lands.
    pub fn maybe_download_attach_bot_icons(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(Vec::new());
        }
        let mut extras = Vec::new();
        for file_id in self.session.attach_bot_icons_to_download() {
            if let Some(extra) = self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY)? {
                extras.push(extra);
            }
        }
        Ok(extras)
    }
}
