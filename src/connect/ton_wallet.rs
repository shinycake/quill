//! Connect driver: the read-only TON wallet (`crate::ton_wallet`). The only
//! request is `getApplicationConfig`, for the explorer address.
use super::*;
use crate::state::RequestPurpose;

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask `getApplicationConfig` once per session for
    /// `ton_blockchain_explorer_url`, the prefix "View in Explorer" opens
    /// (Telegram Desktop's `TonAddressUrl`). `Ok(false)` when already asked.
    pub fn fetch_ton_explorer_url(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.ton_wallet.explorer_url_requested {
            return Ok(false);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetTonExplorerUrl, None);
        let json = crate::telegram::requests::get_application_config(extra);
        self.send_json_request(extra, &json)?;
        self.session.ton_wallet.explorer_url_requested = true;
        Ok(true)
    }
}
