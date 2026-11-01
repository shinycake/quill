//! Connect driver: "Save as HTML" for a rich message
//! (`parity:menu-save-selection-html`). Each export runs on its own worker
//! thread; its TDLib requests go out from the poll loop one at a time, like
//! the chat export's.
use super::*;
use crate::rich_html::{PageLabels, RichHtmlExport, RichHtmlOutcome, RichHtmlRequest};
use std::path::PathBuf;

impl<S: JsonSender> ConnectDriver<S> {
    /// Start saving `message_id` of `chat_id` under `base_dir`; without
    /// one (no download folder), under the account's own `exports` folder,
    /// as tdesktop falls back to the session's temp folder
    /// (`File::DefaultDownloadPath`). Saving the same message again while
    /// it is still being saved does nothing (tdesktop
    /// `Instance::exportRichMessageHtml`). Protected chats are refused:
    /// their messages may not be saved.
    pub fn start_rich_html_export(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        base_dir: Option<PathBuf>,
    ) -> std::io::Result<()> {
        if !self.chats_path_active() {
            return Err(std::io::Error::other("not signed in"));
        }
        if self.session.chat_has_protected_content(chat_id) {
            return Err(std::io::Error::other("the chat's content is protected"));
        }
        let exports = &self.session.settings.rich_html_exports;
        if exports
            .iter()
            .any(|export| export.saves(chat_id, message_id) && export.outcome().is_none())
        {
            return Ok(());
        }
        let fallback_title = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        let base_dir = base_dir.unwrap_or_else(|| self.paths.exports.clone());
        let export = RichHtmlExport::start(
            RichHtmlRequest {
                chat_id,
                message_id,
                base_dir,
                fallback_title,
                labels: PageLabels::current(),
            },
            self.tdlib_files().to_path_buf(),
        )?;
        self.session.settings.rich_html_exports.push(export);
        Ok(())
    }

    /// Send each export's waiting TDLib request. Called from the poll loop.
    pub fn pump_rich_html_exports(&mut self) -> bool {
        let mut taken = false;
        for index in 0..self.session.settings.rich_html_exports.len() {
            taken |= self.pump_export(RequestPurpose::ExportRichHtml, |session| {
                session
                    .settings
                    .rich_html_exports
                    .get_mut(index)
                    .map(|export| &mut export.link)
            });
        }
        taken
    }

    /// The exports that ended since the last call, removed from the
    /// session, oldest first.
    pub fn take_finished_rich_html_exports(&mut self) -> Vec<RichHtmlOutcome> {
        let mut finished = Vec::new();
        self.session
            .settings
            .rich_html_exports
            .retain(|export| match export.outcome() {
                Some(outcome) => {
                    finished.push(outcome);
                    false
                }
                None => true,
            });
        finished
    }
}
