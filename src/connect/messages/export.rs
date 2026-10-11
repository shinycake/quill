//! Connect driver: account and chat export request pumps.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    pub fn start_account_export(
        &mut self,
        folder: &std::path::Path,
        media: bool,
    ) -> std::io::Result<()> {
        if !self.chats_path_active() || self.session.settings.account_export.is_some() {
            return Err(std::io::Error::other(
                "Account export is unavailable or already running",
            ));
        }
        self.session.settings.account_export = Some(crate::account_export::AccountExport::start(
            folder,
            self.tdlib_files().to_path_buf(),
            media,
        )?);
        Ok(())
    }
    pub fn pump_account_export(&mut self) {
        if let Some(mut request) = self
            .session
            .settings
            .account_export
            .as_ref()
            .and_then(|export| export.next_request())
        {
            let extra = self.session.request(RequestPurpose::ExportAccount, None);
            request["@extra"] = serde_json::json!({"quill_account_export":extra.as_extra()});
            if let Some(export) = self.session.settings.account_export.as_mut() {
                export.pending = Some(extra);
            }
            if self.sender.send_json(&request.to_string()).is_err() {
                self.session.requests.take(extra);
                if let Some(export) = self.session.settings.account_export.as_mut() {
                    export.fail_send(extra);
                }
            }
        }
    }

    /// `parity:data-chat-export-html` — start exporting a chat's history to
    /// a folder of HTML and/or JSON with media. Refuses while another chat
    /// export is on screen, and for chats whose content is protected.
    pub fn start_chat_export(
        &mut self,
        settings: crate::export::ExportSettings,
    ) -> std::io::Result<()> {
        if !self.chats_path_active() || self.session.settings.chat_export.is_some() {
            return Err(std::io::Error::other(
                "Another chat export is already running",
            ));
        }
        // Protected chats can't be saved or forwarded, so they can't be
        // exported either (tdesktop `PeerData::canExportChatHistory`
        // requires `allowsForwarding()`).
        if self
            .session
            .chat_has_protected_content(ChatId(settings.chat_id))
        {
            return Err(std::io::Error::other(
                "This chat's content is protected and can't be exported",
            ));
        }
        self.session.settings.chat_export = Some(crate::export::ChatExport::start(
            settings,
            self.tdlib_files().to_path_buf(),
        )?);
        Ok(())
    }

    /// Forward the chat export worker's next TDLib request, if any. Called
    /// from the app's poll loop; a no-op without an export.
    pub fn pump_chat_export(&mut self) {
        let Some(mut request) = self
            .session
            .settings
            .chat_export
            .as_ref()
            .filter(|export| export.pending.is_none())
            .and_then(|export| export.next_request())
        else {
            return;
        };
        let extra = self.session.request(RequestPurpose::ExportChat, None);
        request["@extra"] = serde_json::json!({"quill_account_export": extra.as_extra()});
        if let Some(export) = self.session.settings.chat_export.as_mut() {
            export.pending = Some(extra);
        }
        if self.sender.send_json(&request.to_string()).is_err() {
            self.session.requests.take(extra);
            if let Some(export) = self.session.settings.chat_export.as_mut() {
                export.fail_send(extra);
            }
        }
    }
}
