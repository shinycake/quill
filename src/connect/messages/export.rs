//! Connect driver: account and chat export paging.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    pub fn start_account_export(
        &mut self,
        folder: &std::path::Path,
        media: bool,
    ) -> std::io::Result<()> {
        if !self.chats_path_active() || self.session.account_export.is_some() {
            return Err(std::io::Error::other(
                "Account export is unavailable or already running",
            ));
        }
        self.session.account_export = Some(crate::account_export::AccountExport::start(
            folder,
            self.tdlib_files().to_path_buf(),
            media,
        )?);
        Ok(())
    }
    pub fn pump_account_export(&mut self) {
        if let Some(mut request) = self
            .session
            .account_export
            .as_ref()
            .and_then(|export| export.next_request())
        {
            let extra = self.session.request(RequestPurpose::ExportAccount, None);
            request["@extra"] = serde_json::json!({"quill_account_export":extra.as_extra()});
            if let Some(export) = self.session.account_export.as_mut() {
                export.pending = Some(extra);
            }
            if self.sender.send_json(&request.to_string()).is_err() {
                self.session.requests.take(extra);
                if let Some(export) = self.session.account_export.as_mut() {
                    export.fail_send(extra);
                }
            }
        }
    }

    /// `parity:platform-chat-export` — start exporting a chat's history to
    /// a JSON or HTML file. Refuses while another export is running.
    pub fn start_chat_export(
        &mut self,
        chat_id: ChatId,
        chat_title: String,
        options: crate::chat_export::ChatExportOptions,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.chat_export.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Protected chats can't be saved or forwarded, so they can't be
        // exported either (tdesktop `PeerData::canExportChatHistory`
        // requires `allowsForwarding()`).
        if self.session.chat_has_protected_content(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chat_export = Some(crate::chat_export::ChatExportState::with_options(
            chat_id,
            chat_title,
            options,
            crate::local_time::now_unix(),
        ));
        self.send_export_page()
    }

    /// `parity:platform-chat-export` — drive the export forward: send the
    /// next page while history remains, write the file when paging is
    /// done. Called from the app's live poll loop; no-op without an export.
    pub fn pump_chat_export(&mut self) {
        let done_paging = self
            .session
            .chat_export
            .as_ref()
            .is_some_and(|e| e.done_paging && !e.settled());
        if done_paging && let Some(export) = self.session.chat_export.as_mut() {
            let dir = crate::chat_export::default_export_dir();
            match crate::chat_export::write_export(export, &dir) {
                Ok(path) => export.finished_path = Some(path),
                Err(err) => export.failed = Some(format!("could not write export file: {err}")),
            }
            return;
        }
        let need_page = self
            .session
            .chat_export
            .as_ref()
            .is_some_and(|e| !e.in_flight && !e.done_paging && !e.settled());
        if need_page
            && self.send_export_page().is_err()
            && let Some(export) = self.session.chat_export.as_mut()
        {
            export.failed = Some("failed to send TDLib request".into());
        }
    }

    /// Send one export `getChatHistory` page for the active export.
    fn send_export_page(&mut self) -> Result<(), ConnectSendError> {
        let (chat_id, from) = {
            let export = self
                .session
                .chat_export
                .as_ref()
                .ok_or(ConnectSendError::InvalidRequest)?;
            (export.chat_id, export.page_from())
        };
        let extra = self
            .session
            .request(RequestPurpose::ExportChatHistory, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            from,
            0,
            crate::chat_export::EXPORT_PAGE_LIMIT,
            false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(export) = self.session.chat_export.as_mut() {
            export.in_flight = true;
        }
        Ok(())
    }
}
