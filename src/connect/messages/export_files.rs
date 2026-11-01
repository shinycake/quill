//! Connect driver: deleting the cache copies exports downloaded for
//! themselves, including those an earlier run left (`export::leftovers`).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// An export is done with its cache copy of `file_id`. The copy stays
    /// for a view that asked for it, and while the file's chat is on
    /// screen, so a row that loads there does not fetch it again. The
    /// worker was answered already, so it does not wait for any of this.
    pub(crate) fn release_export_file(&mut self, file_id: i32, chat: Option<i64>) {
        let files = &mut self.session.settings.export_files;
        if files.view_keeps(file_id) {
            files.kept(file_id);
        } else if let Some(chat) = chat.filter(|chat| self.session.chat_is_shown(ChatId(*chat))) {
            self.session.settings.export_files.hold(chat, file_id);
        } else {
            self.send_export_delete(file_id);
        }
    }

    /// Send the held deletes whose chat left the screen, or all of them
    /// once no export runs. Returns whether any went.
    pub(crate) fn release_held_export_deletes(&mut self) -> bool {
        let session = &mut self.session;
        if !session.settings.export_files.holds_any() {
            if session.settings.export_links().next().is_none() {
                session.settings.export_files.forget_view_files();
            }
            return false;
        }
        let exporting = session.settings.exporting();
        let shown = session.chats_shown();
        let shown = |chat: i64| shown.contains(&ChatId(chat));
        let files = session.settings.export_files.release_held(shown, exporting);
        let any = !files.is_empty();
        for file_id in files {
            self.send_export_delete(file_id);
        }
        any
    }

    fn send_export_delete(&mut self, file_id: i32) {
        let request = serde_json::json!({"@type": "deleteFile", "file_id": file_id});
        if let Some(sent) = self.send_export_cleanup(request) {
            self.session.settings.export_files.deleting(sent, file_id);
        }
    }

    /// On Ready: ask TDLib about each file an earlier run's export left in
    /// the cache. File ids change between runs, so the remote id finds it.
    pub(crate) fn check_export_leftovers(&mut self) {
        for earlier in self.session.settings.export_files.earlier() {
            let request = serde_json::json!({"@type": "getRemoteFile",
                "remote_file_id": earlier.entry.remote_id});
            if let Some(sent) = self.send_export_cleanup(request) {
                self.session.settings.export_files.checking(sent, earlier);
            }
        }
    }

    /// Delete the earlier files found untouched. Returns whether any went.
    pub(crate) fn send_due_export_deletes(&mut self) -> bool {
        let due = self.session.settings.export_files.take_due();
        let any = !due.is_empty();
        for (file_id, name) in due {
            let request = serde_json::json!({"@type": "deleteFile", "file_id": file_id});
            if let Some(sent) = self.send_export_cleanup(request) {
                self.session
                    .settings
                    .export_files
                    .deleting_entry(sent, name);
            }
        }
        any
    }

    fn send_export_cleanup(&mut self, mut request: serde_json::Value) -> Option<RequestId> {
        let extra = self.session.request(RequestPurpose::ExportCleanup, None);
        crate::json_access::set(
            &mut request,
            "@extra",
            serde_json::json!({"quill_account_export": extra.as_extra()}),
        );
        if self.sender.send_json(&request.to_string()).is_err() {
            self.session.requests.take(extra);
            return None;
        }
        Some(extra)
    }
}
