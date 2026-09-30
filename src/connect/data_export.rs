//! Connect driver: full account data export (`parity:platform-data-export`).
//!
//! The export runs in four phases, all pumped from the app's poll loop so
//! the UI thread never blocks:
//!
//! 1. contacts — wait for the (deduped) `getContacts` fetch when opted in;
//! 2. chats — page every non-secret chat through the shared per-chat
//!    `ChatExportState` machinery, one chat at a time, writing each chat's
//!    JSON (the `chat_export` format) into the bundle's `chats/`;
//! 3. media — download queued media files (concurrency-capped) and copy
//!    finished files into the bundle's `media/`;
//! 4. finalize — write `account.json` / `contacts.json` /
//!    `media_manifest.json`.
use super::*;
use crate::data_export::{DataExportOptions, DataExportState, new_bundle_dir};
use crate::ids::{ChatId, FileId};
use crate::telegram::envelope::ChatKind;
use std::path::PathBuf;

/// Simultaneous export media downloads. Above thumbs/auto-media (4 and 1)
/// would starve the live UI's own downloads; far below explicit user
/// downloads (32).
const EXPORT_MEDIA_CONCURRENCY: usize = 4;

impl<S: JsonSender> ConnectDriver<S> {
    /// Start a full account data export into a new bundle directory inside
    /// `dest`. Returns the bundle directory. Refuses while any export is
    /// running or the client is not connected.
    pub fn start_data_export(
        &mut self,
        options: DataExportOptions,
        dest: PathBuf,
    ) -> Result<PathBuf, String> {
        if !self.chats_path_active() {
            return Err("not connected".to_string());
        }
        if self.session.data_export.is_some() || self.session.chat_export.is_some() {
            return Err("an export is already running".to_string());
        }
        let dir = new_bundle_dir(&dest)
            .map_err(|err| format!("could not create export folder: {err}"))?;
        let mut dx = DataExportState::new(options, dir.clone());
        if options.chats {
            let mut chats: Vec<(ChatId, String)> = Vec::new();
            let mut secret = 0;
            for chat in self.session.chats.values() {
                if matches!(chat.kind, ChatKind::Secret { .. }) {
                    secret += 1;
                } else {
                    chats.push((chat.id, chat.title.clone()));
                }
            }
            dx.set_queue(chats, secret);
        }
        // The account holder's profile photo is just another media job
        // (chat_id/message_id 0 mark it as account-level).
        if let Some(my_id) = self.session.my_user_id
            && let Some(me) = self.session.users.get(&my_id)
        {
            dx.enqueue_profile_photo(me.photo_small_file_id);
        }
        self.session.data_export = Some(dx);
        if options.contacts && self.session.contacts.is_none() {
            let _ = self.fetch_contacts();
        }
        Ok(dir)
    }

    /// Cancel a running account export. In-flight media downloads keep
    /// going in the background (they land in TDLib's cache, harmless).
    pub fn cancel_data_export(&mut self) {
        self.session.data_export = None;
        self.session.chat_export = None;
    }

    /// Drive the export forward: contacts gate, chat sequencing, media
    /// drain, finalize. Called from the app's live poll loop; no-op
    /// without an active export.
    pub fn pump_data_export(&mut self) {
        if !self
            .session
            .data_export
            .as_ref()
            .is_some_and(|dx| !dx.settled())
        {
            return;
        }
        // A failed chat page aborts the whole batch with the chat's error.
        if let Some(msg) = self
            .session
            .chat_export
            .as_ref()
            .and_then(|e| e.failed.clone())
        {
            self.session.chat_export = None;
            if let Some(dx) = self.session.data_export.as_mut() {
                dx.failed = Some(format!("chat export failed: {msg}"));
            }
            return;
        }
        // Phase 1: contacts must be loaded (or have failed) first.
        if self.poll_export_contacts() {
            return;
        }
        // Phase 2: page chats one at a time through the shared machinery.
        let chats_pending = self
            .session
            .data_export
            .as_ref()
            .is_some_and(|dx| !dx.queue.is_empty());
        if self.session.chat_export.is_none() && chats_pending {
            let next = self
                .session
                .data_export
                .as_mut()
                .and_then(|dx| dx.queue.pop());
            if let Some((chat_id, title)) = next
                && let Err(err) = self.start_chat_export(chat_id, title)
                && let Some(dx) = self.session.data_export.as_mut()
            {
                dx.failed = Some(format!("could not start chat export: {err:?}"));
            }
            return;
        }
        // Phase 3: drain media downloads once every chat is paged.
        let media_pending = self
            .session
            .data_export
            .as_ref()
            .is_some_and(|dx| !dx.media_jobs.is_empty());
        if self.session.chat_export.is_none() && !chats_pending && media_pending {
            self.pump_export_media();
        }
        // Phase 4: finalize the bundle.
        let ready = self.session.chat_export.is_none()
            && !chats_pending
            && self
                .session
                .data_export
                .as_ref()
                .is_some_and(|dx| dx.media_jobs.is_empty() && !dx.settled());
        if ready {
            let result = {
                let session = &self.session;
                session
                    .data_export
                    .as_ref()
                    .expect("checked")
                    .finalize(session)
            };
            let dx = self.session.data_export.as_mut().expect("checked");
            match result {
                Ok(()) => dx.finished = true,
                Err(note) => dx.failed = Some(note),
            }
        }
    }

    /// Contacts gate: fire the (deduped) fetch and wait. Returns true
    /// while the export must wait for contacts.
    fn poll_export_contacts(&mut self) -> bool {
        let waiting = self.session.data_export.as_ref().is_some_and(|dx| {
            dx.options.contacts && !dx.contacts_fetched && self.session.contacts.is_none()
        });
        if !waiting {
            return false;
        }
        if self.session.contacts_error {
            if let Some(dx) = self.session.data_export.as_mut() {
                dx.contacts_fetched = true;
                dx.contacts_note = Some("the contacts list could not be loaded".to_string());
            }
            return false;
        }
        let _ = self.fetch_contacts();
        true
    }

    /// Sweep finished/failed media jobs (copying completed files into the
    /// bundle) and top downloads back up to the concurrency cap.
    fn pump_export_media(&mut self) {
        enum Outcome {
            Done(std::path::PathBuf),
            Failed,
        }
        // Pass 1 (immutable borrow): decide each job's outcome.
        let mut resolved: Vec<(usize, Outcome)> = Vec::new();
        {
            let session = &self.session;
            let dx = session.data_export.as_ref().expect("checked");
            for (ix, job) in dx.media_jobs.iter().enumerate() {
                let file_id = FileId(job.file_id);
                if let Some(path) = session.file(file_id).and_then(|f| f.usable_path()) {
                    resolved.push((ix, Outcome::Done(path.into())));
                    continue;
                }
                let failed = session.failed_downloads.contains(&job.file_id)
                    || session.file(file_id).is_some_and(|f| {
                        !f.local.can_be_downloaded && !session.downloading.contains(&job.file_id)
                    });
                if failed {
                    resolved.push((ix, Outcome::Failed));
                }
            }
        }
        // Pass 2: apply, highest index first so removals don't shift.
        resolved.sort_by_key(|(ix, _)| std::cmp::Reverse(*ix));
        for (ix, outcome) in resolved {
            let (job, dest) = {
                let dx = self.session.data_export.as_mut().expect("checked");
                let job = dx.media_jobs.remove(ix);
                let dest = dx.media_dest(&job);
                (job, dest)
            };
            match outcome {
                Outcome::Done(src) => match std::fs::copy(&src, &dest) {
                    Ok(_) => {
                        let dx = self.session.data_export.as_mut().expect("checked");
                        dx.media_done += 1;
                        dx.media_manifest
                            .push(crate::data_export::MediaManifestEntry {
                                chat_id: job.chat_id,
                                message_id: job.message_id,
                                file: dest
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                            });
                    }
                    Err(_) => {
                        self.session
                            .data_export
                            .as_mut()
                            .expect("checked")
                            .media_failed += 1;
                    }
                },
                Outcome::Failed => {
                    self.session
                        .data_export
                        .as_mut()
                        .expect("checked")
                        .media_failed += 1;
                }
            }
        }
        // Pass 3: start downloads up to the cap.
        loop {
            let in_flight = {
                let session = &self.session;
                let dx = session.data_export.as_ref().expect("checked");
                dx.media_jobs
                    .iter()
                    .filter(|job| session.downloading.contains(&job.file_id))
                    .count()
            };
            if in_flight >= EXPORT_MEDIA_CONCURRENCY {
                break;
            }
            let next = {
                let session = &self.session;
                let dx = session.data_export.as_ref().expect("checked");
                dx.media_jobs
                    .iter()
                    .find(|job| {
                        let file_id = FileId(job.file_id);
                        !session.downloading.contains(&job.file_id)
                            && session
                                .file(file_id)
                                .and_then(|f| f.usable_path())
                                .is_none()
                            && session.should_download(file_id)
                    })
                    .map(|job| job.file_id)
            };
            let Some(file_id) = next else { break };
            match self.download_file(FileId(file_id), AUTO_MEDIA_DOWNLOAD_PRIORITY) {
                Ok(Some(_)) => {}
                // The gate refused after all (already local/in flight) —
                // stop this tick rather than spinning on the same job.
                Ok(None) => break,
                Err(_) => {
                    // Send failed (went offline?) — park the job as
                    // failed rather than stalling the whole export.
                    let dx = self.session.data_export.as_mut().expect("checked");
                    if let Some(pos) = dx.media_jobs.iter().position(|j| j.file_id == file_id) {
                        dx.media_jobs.remove(pos);
                        dx.media_failed += 1;
                    }
                }
            }
        }
    }
}
