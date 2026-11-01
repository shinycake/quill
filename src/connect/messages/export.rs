//! Connect driver: account and chat export request pumps.
use super::*;
use crate::export::handle::WorkerLink;
use crate::state::Session;

impl<S: JsonSender> ConnectDriver<S> {
    /// `parity:data-export-full` / `parity:platform-data-export` — start
    /// the full account export with the "Export Telegram data" box's
    /// choices. Refuses while another account export is on screen.
    pub fn start_account_export(
        &mut self,
        settings: crate::export::AccountSettings,
    ) -> std::io::Result<()> {
        if !self.chats_path_active() || self.session.settings.account_export.is_some() {
            return Err(std::io::Error::other(
                "An export of your data is already running",
            ));
        }
        if !settings.can_start() {
            return Err(std::io::Error::other("Choose something to export"));
        }
        self.session.settings.account_export = Some(crate::export::AccountExport::start(
            settings,
            self.tdlib_files().to_path_buf(),
        )?);
        Ok(())
    }

    /// Forward the account export worker's next TDLib request, if any.
    /// Called from the app's poll loop; a no-op without an export.
    pub fn pump_account_export(&mut self) {
        self.pump_export(RequestPurpose::ExportAccount, |session| {
            session
                .settings
                .account_export
                .as_mut()
                .map(|export| &mut export.link)
        });
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
        self.pump_export(RequestPurpose::ExportChat, |session| {
            session
                .settings
                .chat_export
                .as_mut()
                .map(|export| &mut export.link)
        });
    }

    /// The view asked for `file_id`: a running export must not delete it
    /// from TDLib's cache after copying it.
    pub(crate) fn note_view_file(&mut self, file_id: crate::ids::FileId) {
        let settings = &mut self.session.settings;
        if let Some(export) = settings.chat_export.as_mut() {
            export.link.note_view_file(file_id.0);
        }
        if let Some(export) = settings.account_export.as_mut() {
            export.link.note_view_file(file_id.0);
        }
    }

    pub(crate) fn pump_exports(&mut self) {
        self.pump_chat_export();
        self.pump_account_export();
        self.pump_rich_html_exports();
    }

    pub(super) fn pump_export(
        &mut self,
        purpose: RequestPurpose,
        link: impl Fn(&mut Session) -> Option<&mut WorkerLink>,
    ) {
        let Some(mut request) = link(&mut self.session)
            .filter(|link| link.idle_or_cancelled())
            .and_then(|link| link.next_request())
        else {
            return;
        };
        if link(&mut self.session).is_some_and(|link| link.keep_view_file(&request)) {
            return;
        }
        let extra = self.session.request(purpose, None);
        crate::json_access::set(
            &mut request,
            "@extra",
            serde_json::json!({"quill_account_export": extra.as_extra()}),
        );
        if let Some(link) = link(&mut self.session) {
            link.pending = Some(extra);
        }
        if self.sender.send_json(&request.to_string()).is_err() {
            self.session.requests.take(extra);
            if let Some(link) = link(&mut self.session) {
                link.fail_send(extra);
            }
        }
    }
}
