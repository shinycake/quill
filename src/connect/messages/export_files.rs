//! Connect driver: deleting the cache copies exports downloaded for
//! themselves, including those an earlier run left (`export::leftovers`).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
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
