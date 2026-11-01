//! Connect driver: account and chat export request pumps.
use super::*;
use crate::export::handle::WorkerLink;
use crate::json_access::JsonExt;
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
            self.session.settings.export_files.leftovers().cloned(),
        )?);
        Ok(())
    }

    /// Forward the account export worker's next TDLib request, if any.
    /// Called from the app's poll loop; a no-op without an export. Returns
    /// whether one was taken.
    pub fn pump_account_export(&mut self) -> bool {
        self.pump_export(RequestPurpose::ExportAccount, |session| {
            session
                .settings
                .account_export
                .as_mut()
                .map(|export| &mut export.link)
        })
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
            self.session.settings.export_files.leftovers().cloned(),
        )?);
        Ok(())
    }

    /// Forward the chat export worker's next TDLib request, if any. Called
    /// from the app's poll loop; a no-op without an export. Returns whether
    /// one was taken.
    pub fn pump_chat_export(&mut self) -> bool {
        self.pump_export(RequestPurpose::ExportChat, |session| {
            session
                .settings
                .chat_export
                .as_mut()
                .map(|export| &mut export.link)
        })
    }

    /// The view asked for `file_id`: a running export must not delete it
    /// from TDLib's cache after copying it.
    pub(crate) fn note_view_file(&mut self, file_id: crate::ids::FileId) {
        let settings = &mut self.session.settings;
        if settings.export_links().next().is_some() || settings.export_files.holds_any() {
            settings.export_files.note_view_file(file_id.0);
        }
    }

    pub(crate) fn pump_exports(&mut self) -> bool {
        let chat = self.pump_chat_export();
        let account = self.pump_account_export();
        let rich_html = self.pump_rich_html_exports();
        let held = self.release_held_export_deletes();
        chat | account | rich_html | held | self.send_due_export_deletes()
    }

    /// `close` comes next and TDLib takes nothing after it, so what the
    /// workers left queued goes now: a cancelled download, the cache delete
    /// of a file just saved.
    pub(crate) fn flush_export_requests(&mut self) {
        while self.pump_exports() {}
    }

    pub(super) fn pump_export(
        &mut self,
        purpose: RequestPurpose,
        link: impl Fn(&mut Session) -> Option<&mut WorkerLink>,
    ) -> bool {
        let Some(mut request) = link(&mut self.session)
            .filter(|link| link.idle_or_cancelled())
            .and_then(|link| link.next_request())
        else {
            return false;
        };
        if let Some(file_id) = deleted_file(&request) {
            if let Some(link) = link(&mut self.session) {
                link.answer_ok();
            }
            let chat = request.at("chat_id").as_i64();
            self.release_export_file(file_id, chat);
            return true;
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
        true
    }
}

fn deleted_file(request: &serde_json::Value) -> Option<i32> {
    if request.at("@type") != "deleteFile" {
        return None;
    }
    i32::try_from(request.at("file_id").as_i64()?).ok()
}
