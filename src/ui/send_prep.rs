//! Sending photos, videos and round videos without waiting for ffmpeg.
//!
//! A media send reads its files first: `ffprobe` for a video's length and
//! size, `ffmpeg` to scale a large photo down. That used to run on the UI
//! thread inside the send, one process per file. Now Send hands the
//! snapshots to the background executor and returns: the tray and the text
//! stay where they are, marked as preparing (derived from
//! [`SendPrepState`], never from a flag the screen keeps), and the send goes
//! out when the files are ready, to the chat and account Send was tapped in,
//! whichever chat is open by then. A slot has at most one send in flight: a
//! Send tapped in the same chat meanwhile is refused with the "Preparing the
//! files" note (it does not queue), so the messages keep their order.
//!
//! A failed or timed out preparation changes nothing but the toast: the
//! draft, the tray and the reply are still there for another try. After a
//! successful send only what Send took is cleared: text typed or files
//! added meanwhile stay. When the driver refuses a later message after
//! earlier ones went out, exactly the snapshots that went out are cleared
//! (their files from the tray, their text from the draft), what is left stays
//! for another try, and the toast says how many went.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::composer_sends::SendSlot;
use super::composer_state::DraftClear;
use super::status_note::StatusNote;
use super::user_error::user_error;
use gpui_kit::*;
use quill::composer::{ComposerSnapshot, PrepareError, PreparedMedia};
use quill::ids::{AccountKey, ChatId};
use quill::lang::{keys, tr, tr_with};
use std::path::PathBuf;
use std::time::Instant;

/// The slots with a send being prepared.
#[derive(Default)]
pub(crate) struct SendPrepState {
    next_id: u64,
    jobs: Vec<PrepJob>,
}

struct PrepJob {
    id: u64,
    slot: SendSlot,
}

impl SendPrepState {
    /// Start a job for `slot`: its number, or `None` when the slot already
    /// has one (the caller refuses that Send, so the sends of one chat go out
    /// in the order they were made).
    pub(super) fn begin(&mut self, slot: SendSlot) -> Option<u64> {
        if self.is_preparing(&slot) {
            return None;
        }
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.jobs.push(PrepJob { id, slot });
        Some(id)
    }

    /// End job `id`: the slot it was for, or `None` when it was cancelled
    /// meanwhile (its account left) and must not send.
    pub(super) fn finish(&mut self, id: u64) -> Option<SendSlot> {
        let at = self.jobs.iter().position(|job| job.id == id)?;
        Some(self.jobs.remove(at).slot)
    }

    pub(super) fn is_preparing(&self, slot: &SendSlot) -> bool {
        self.jobs.iter().any(|job| &job.slot == slot)
    }

    /// The account logged out or was removed: its jobs end unsent.
    pub(super) fn cancel_account(&mut self, account: &AccountKey) {
        self.jobs.retain(|job| &job.slot.account != account);
    }
}

/// Where a send goes and what Send took, fixed when Send is tapped.
#[derive(Debug, Clone)]
pub(super) struct SendOrigin {
    pub(super) slot: SendSlot,
    pub(super) chat_id: ChatId,
    /// The composer's markup when Send was tapped.
    pub(super) markup: String,
    /// The files of the snapshots.
    pub(super) files: Vec<PathBuf>,
}

impl SendOrigin {
    pub(super) fn new(
        slot: SendSlot,
        chat_id: ChatId,
        markup: String,
        snaps: &[ComposerSnapshot],
    ) -> Self {
        let files = snaps
            .iter()
            .flat_map(|snap| snap.attachment.iter().chain(snap.album.iter()))
            .map(|attachment| attachment.path.clone())
            .collect();
        Self {
            slot,
            chat_id,
            markup,
            files,
        }
    }
}

/// The error toast of a send that stopped after `sent` of `total` messages.
pub(in crate::ui) fn partly_sent_note(sent: usize, total: usize) -> StatusNote {
    StatusNote::error(tr_with(
        keys::Q_CHAT_PARTLY_SENT,
        &[("sent", &sent.to_string()), ("total", &total.to_string())],
    ))
}

/// What a successful send clears from its slot.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct SentCleanup {
    /// The text is still what Send took: clear it (and its stored draft).
    pub(super) clear_text: bool,
    /// The tray holds nothing but what was sent: the slot starts over.
    /// Otherwise only the sent files leave it.
    pub(super) reset_slot: bool,
}

impl SentCleanup {
    /// Whether the stored draft is cleared when the send is acknowledged:
    /// only when it is what was sent. Unsent parts that were kept, or text
    /// typed since, are not.
    pub(super) fn clears_draft_on_ack(&self, left: &str) -> bool {
        self.clear_text && left.is_empty()
    }
}

/// `text_now` is the composer's markup (or the slot's stored draft when it
/// is not on screen); `tray_now` the slot's files.
pub(super) fn plan_cleanup(
    origin: &SendOrigin,
    text_now: Option<&str>,
    tray_now: &[PathBuf],
) -> SentCleanup {
    SentCleanup {
        clear_text: text_now.is_some_and(|now| now == origin.markup),
        reset_slot: tray_now.iter().all(|file| origin.files.contains(file)),
    }
}

/// What is left in the composer text after the first `sent` snapshots went
/// out: a split long text keeps its unsent parts, cut from the original
/// `markup` so the characters at the seams stay (the splitter trims them); a
/// media caption rides the first snapshot, so once that one is out nothing of
/// it is left.
pub(super) fn unsent_text(markup: &str, snaps: &[ComposerSnapshot], sent: usize) -> String {
    let rest = snaps.get(sent..).unwrap_or_default();
    if rest
        .iter()
        .any(|snap| snap.attachment.is_some() || !snap.album.is_empty())
    {
        return String::new();
    }
    let Some(first) = rest.first() else {
        return String::new();
    };
    let whole = markup.trim();
    // The parts are consecutive pieces of the trimmed markup: find where the
    // first unsent one starts after the sent ones.
    let mut at = 0usize;
    for snap in snaps.iter().take(sent) {
        let found = whole
            .get(at..)
            .and_then(|tail| tail.find(snap.text.as_str()));
        match found {
            Some(found) => at += found + snap.text.len(),
            None => return joined_parts(rest),
        }
    }
    let start = whole
        .get(at..)
        .and_then(|tail| tail.find(first.text.as_str()))
        .map(|found| at + found);
    match start {
        Some(start) => whole.get(start..).unwrap_or_default().to_string(),
        None => joined_parts(rest),
    }
}

/// The parts one per line: the fallback when the markup cannot be matched.
fn joined_parts(parts: &[ComposerSnapshot]) -> String {
    parts
        .iter()
        .map(|snap| snap.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Read the files of every snapshot that needs it with `prepare` (off the UI
/// thread). `prepare` gets the moment the whole send started preparing, so
/// one budget covers all the snapshots. The first failure ends it.
pub(super) fn prepare_snapshots(
    snaps: Vec<ComposerSnapshot>,
    prepare: impl Fn(&ComposerSnapshot, Instant) -> Result<PreparedMedia, PrepareError>,
) -> Result<Vec<ComposerSnapshot>, PrepareError> {
    let started = Instant::now();
    snaps
        .into_iter()
        .map(|snap| {
            if snap.needs_media_prep() {
                let prepared = prepare(&snap, started)?;
                Ok(snap.with_prepared(prepared))
            } else {
                Ok(snap)
            }
        })
        .collect()
}

impl QuillApp {
    /// Files of the open slot's send are being prepared.
    pub(super) fn tray_preparing(&self) -> bool {
        self.send_slot()
            .is_some_and(|slot| self.composer_ui.send_box.prep.is_preparing(&slot))
    }

    /// Read the files in the background, then send. Returns at once.
    pub(super) fn begin_media_prep(
        &mut self,
        origin: SendOrigin,
        snaps: Vec<ComposerSnapshot>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = self.composer_ui.send_box.prep.begin(origin.slot.clone()) else {
            return;
        };
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_OPS_PREPARING_THE_FILES)));
        cx.notify();
        let worker =
            cx.background_spawn(async move { prepare_snapshots(snaps, PreparedMedia::prepare) });
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let prepared = worker.await;
            let ran = this.update_in(cx, |this, window, cx| {
                this.finish_media_prep(job, origin, prepared, window, cx);
            });
            if ran.is_err() {
                // The window closed: the job must not hold the slot.
                let _ = this.main().update(cx, |this, cx| {
                    this.composer_ui.send_box.prep.finish(job);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn finish_media_prep(
        &mut self,
        job: u64,
        origin: SendOrigin,
        prepared: Result<Vec<ComposerSnapshot>, PrepareError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composer_ui.send_box.prep.finish(job).is_none() {
            return;
        }
        match prepared {
            Ok(snaps) => self.deliver_live_send(origin, snaps, window, cx),
            Err(err) => {
                // Nothing was taken from the composer: the draft stays.
                self.connection
                    .set_note(StatusNote::error(user_error(&err)));
                cx.notify();
            }
        }
    }

    /// Hand the snapshots to the driver, one message at a time, and clear
    /// what Send took when they all went out.
    pub(super) fn deliver_live_send(
        &mut self,
        origin: SendOrigin,
        snaps: Vec<ComposerSnapshot>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_account = self
            .session()
            .is_some_and(|session| session.account == origin.slot.account);
        let (true, Some(live), Some(first)) = (same_account, self.live.as_mut(), snaps.first())
        else {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SEND_MESSAGE)));
            cx.notify();
            return;
        };
        let scheduling = first.send_options.scheduling;
        let mut sent = 0usize;
        let mut failure = None;
        for snap in &snaps {
            match live.driver.send_snapshot(snap) {
                Ok(_) => sent += 1,
                Err(err) => {
                    failure = Some(err);
                    break;
                }
            }
        }
        let total = snaps.len();
        let is_open = self.send_slot().as_ref() == Some(&origin.slot);
        if sent > 0 {
            // Clear exactly what went out: the files of the sent snapshots
            // and the text they took. A partial send keeps the rest.
            let sent_snaps = snaps.get(..sent).unwrap_or_default();
            let done = SendOrigin::new(
                origin.slot.clone(),
                origin.chat_id,
                origin.markup.clone(),
                sent_snaps,
            );
            let left = unsent_text(&origin.markup, &snaps, sent);
            self.clear_after_send(&done, is_open, &left, window, cx);
        }
        match failure {
            None => {
                self.connection
                    .set_note(self.send_started_note(scheduling, tr(keys::Q_CHAT_SENDING)));
            }
            Some(quill::connect::ConnectSendError::CaptionTooLong { limit })
                if is_open && sent == 0 =>
            {
                // MED4: runtime `message_caption_length_max` refusal — the
                // counter already warned; this names the limit.
                // tdesktop `CaptionLimitBox`: the limit box says by how
                // much, with Premium's 4096 as the way out. The count is
                // the caption Send took, not what is typed since.
                let over = origin.markup.chars().count() as i64 - i64::from(limit);
                self.show_premium_limit(
                    quill::premium_limits::PremiumLimitKind::Caption,
                    over.max(0),
                    cx,
                );
            }
            Some(_) if sent > 0 => {
                self.connection.set_note(partly_sent_note(sent, total));
            }
            Some(_) => {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SEND_MESSAGE)));
            }
        }
        cx.notify();
    }

    /// The send went: clear what Send took. In the chat on screen that is
    /// the text, the reply and the options; in a chat left meanwhile its
    /// stored draft and tray (tdesktop clears the field and resets the
    /// preview draft after sending).
    fn clear_after_send(
        &mut self,
        origin: &SendOrigin,
        is_open: bool,
        left: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat_id = origin.chat_id;
        let topic = origin.slot.draft.topic;
        let text_now = if is_open {
            Some(self.composer_markup(cx))
        } else {
            self.session()
                .and_then(|session| session.draft_for(chat_id, topic))
                .map(|draft| draft.text.clone())
        };
        let tray_now: Vec<PathBuf> = self
            .composer_sends
            .get(&origin.slot)
            .attachments
            .iter()
            .map(|attachment| attachment.path.clone())
            .collect();
        let plan = plan_cleanup(origin, text_now.as_deref(), &tray_now);
        if plan.reset_slot {
            if let Some(sent) = self.composer_sends.finish(&origin.slot) {
                self.forget_tray_checks(&sent.attachments);
            }
        } else {
            let dropped = self.composer_sends.update(&origin.slot, |send| {
                let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut send.attachments)
                    .into_iter()
                    .partition(|attachment| origin.files.contains(&attachment.path));
                send.attachments = kept;
                gone
            });
            self.forget_tray_checks(&dropped);
        }
        if is_open {
            // MED1: the grouping override was consumed: the next composer
            // follows the pref again.
            self.composer_ui.group_media = None;
            self.composer_ui.schedule_popup_open = false;
            self.composer_ui.set_reply(None);
            self.clear_suggest_bar_after_send(chat_id);
        }
        self.composer_ui.clear_draft_on_success = plan
            .clears_draft_on_ack(left)
            .then(|| DraftClear::of_text(chat_id, &origin.markup));
        if plan.clear_text {
            // `left`: the unsent parts of a split text, else nothing.
            if is_open && left.is_empty() {
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.forget_local_draft(chat_id);
            } else if is_open {
                self.set_composer_markup(left, window, cx);
            } else if left.is_empty() {
                self.forget_stored_draft(chat_id, topic);
            } else {
                self.keep_stored_draft_text(chat_id, topic, left);
            }
        }
        if is_open {
            // `set_value` sends no change event: drop the sticker and
            // animated-emoji suggestions the sent text held.
            let text = self.composer.read(cx).value().to_string();
            self.sync_composer_suggestions(&text, cx);
        }
    }

    /// Cut the stored draft of a chat that is not on screen down to `text`.
    fn keep_stored_draft_text(
        &mut self,
        chat_id: ChatId,
        topic: quill::composer::DraftTopic,
        text: &str,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_pending_draft();
            let draft = live
                .driver
                .session
                .draft_for(chat_id, topic)
                .cloned()
                .map(|mut draft| {
                    draft.text = text.to_string();
                    draft
                });
            live.driver
                .session
                .store_composer_draft(chat_id, topic, draft);
        }
    }

    /// Drop the stored draft of a chat that is not on screen.
    fn forget_stored_draft(&mut self, chat_id: ChatId, topic: quill::composer::DraftTopic) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_pending_draft();
            live.driver
                .session
                .store_composer_draft(chat_id, topic, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DraftClear, PrepareError, PreparedMedia, SendOrigin, SendPrepState, SentCleanup,
        plan_cleanup, prepare_snapshots, unsent_text,
    };
    use crate::ui::composer_sends::SendSlot;
    use quill::composer::{AttachmentKind, ComposerAttachment, ComposerSnapshot, DraftTopic};
    use quill::ids::{AccountKey, ChatId, ViewGeneration};
    use std::path::PathBuf;

    fn slot(account: &str, chat: i64) -> SendSlot {
        SendSlot {
            account: AccountKey(account.to_string()),
            draft: DraftTopic::Chat.key(chat),
        }
    }

    fn photo(path: &str) -> ComposerAttachment {
        ComposerAttachment {
            path: PathBuf::from(path),
            kind: AttachmentKind::Photo,
            file_name: path.into(),
            spoiler: false,
            extras: Default::default(),
        }
    }

    fn photo_snapshot(chat: i64, path: &str) -> ComposerSnapshot {
        ComposerSnapshot::capture_with_attachment(
            ChatId(chat),
            ViewGeneration(0),
            "caption",
            Some(photo(path)),
        )
    }

    #[test]
    fn a_slot_has_one_send_in_flight_and_other_slots_are_free() {
        let mut prep = SendPrepState::default();
        let first = prep.begin(slot("a", 11)).expect("starts");
        assert!(prep.is_preparing(&slot("a", 11)));
        assert!(
            prep.begin(slot("a", 11)).is_none(),
            "a second send in the chat is refused, keeping the order"
        );
        let other = prep.begin(slot("a", 12)).expect("another chat is free");
        assert_eq!(prep.finish(first), Some(slot("a", 11)));
        assert!(!prep.is_preparing(&slot("a", 11)));
        assert!(prep.is_preparing(&slot("a", 12)));
        assert_eq!(prep.finish(other), Some(slot("a", 12)));
        assert_eq!(prep.finish(other), None, "a job ends once");
    }

    #[test]
    fn the_job_keeps_the_chat_it_started_in() {
        let mut prep = SendPrepState::default();
        let job = prep.begin(slot("a", 11)).unwrap();
        // The user opens chat 12 while the files are read: the job still
        // reports chat 11.
        let open_chat = 12;
        let sent_from = prep.finish(job).unwrap();
        assert_eq!(sent_from.draft.chat_id, 11);
        assert_ne!(sent_from.draft.chat_id, open_chat);
    }

    #[test]
    fn a_logged_out_account_cancels_its_jobs() {
        let mut prep = SendPrepState::default();
        let gone = prep.begin(slot("a", 11)).unwrap();
        let kept = prep.begin(slot("b", 11)).unwrap();
        prep.cancel_account(&AccountKey("a".to_string()));
        assert_eq!(prep.finish(gone), None, "nothing is sent for it");
        assert_eq!(prep.finish(kept), Some(slot("b", 11)));
    }

    #[test]
    fn a_slow_preparation_does_not_hold_the_caller() {
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let snaps = vec![photo_snapshot(11, "/x/a.jpg")];
        let worker = std::thread::spawn(move || {
            let result = prepare_snapshots(snaps, |_, _| {
                wait.recv().ok();
                Ok(PreparedMedia::default())
            });
            done_tx.send(result).ok();
        });
        // The caller is free while the worker is stuck in its slow step.
        assert!(done_rx.try_recv().is_err());
        release.send(()).unwrap();
        let sent = done_rx.recv().unwrap().unwrap();
        worker.join().unwrap();
        let snap = sent.first().expect("one snapshot");
        assert_eq!(snap.chat_id, 11, "the chat does not change");
        assert!(snap.prepared.is_some());
    }

    #[test]
    fn snapshots_keep_their_order_and_text_only_ones_skip_the_step() {
        let text = ComposerSnapshot::capture(ChatId(11), ViewGeneration(0), "hi");
        let snaps = vec![
            photo_snapshot(11, "/x/a.jpg"),
            text,
            photo_snapshot(11, "/x/b.jpg"),
        ];
        let calls = std::cell::RefCell::new(Vec::new());
        let out = prepare_snapshots(snaps, |snap, _| {
            calls
                .borrow_mut()
                .push(snap.attachment.as_ref().map(|att| att.path.clone()));
            Ok(PreparedMedia::default())
        })
        .unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(calls.borrow().len(), 2, "text needs no files read");
        assert_eq!(out.get(1).map(|snap| snap.text.as_str()), Some("hi"));
        assert!(out.get(1).is_some_and(|snap| snap.prepared.is_none()));
        assert_eq!(
            out.iter()
                .map(|snap| snap.text.as_str())
                .collect::<Vec<_>>(),
            ["caption", "hi", "caption"]
        );
    }

    #[test]
    fn a_failed_preparation_fails_the_whole_send() {
        let snaps = vec![
            photo_snapshot(11, "/x/a.jpg"),
            photo_snapshot(11, "/x/b.jpg"),
        ];
        let out = prepare_snapshots(snaps, |snap, _| {
            if snap
                .attachment
                .as_ref()
                .is_some_and(|att| att.path.ends_with("b.jpg"))
            {
                Err(PrepareError::TimedOut)
            } else {
                Ok(PreparedMedia::default())
            }
        });
        assert!(matches!(out, Err(PrepareError::TimedOut)));
    }

    fn origin(files: &[&str]) -> SendOrigin {
        SendOrigin {
            slot: slot("a", 11),
            chat_id: ChatId(11),
            markup: "caption".into(),
            files: files.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn a_sent_slot_starts_over_when_nothing_new_was_added() {
        let origin = origin(&["/x/a.jpg", "/x/b.jpg"]);
        let tray = [PathBuf::from("/x/a.jpg"), PathBuf::from("/x/b.jpg")];
        assert_eq!(
            plan_cleanup(&origin, Some("caption"), &tray),
            SentCleanup {
                clear_text: true,
                reset_slot: true
            }
        );
    }

    #[test]
    fn text_and_files_added_meanwhile_are_not_wiped() {
        let origin = origin(&["/x/a.jpg"]);
        let tray = [PathBuf::from("/x/a.jpg"), PathBuf::from("/x/new.jpg")];
        assert_eq!(
            plan_cleanup(&origin, Some("caption and more"), &tray),
            SentCleanup {
                clear_text: false,
                reset_slot: false
            }
        );
    }

    fn snapshots(paths: &[&str]) -> Vec<ComposerSnapshot> {
        paths.iter().map(|path| photo_snapshot(11, path)).collect()
    }

    #[test]
    fn a_partial_send_clears_only_the_sent_files_and_keeps_the_rest() {
        let snaps = snapshots(&["/x/a.jpg", "/x/b.jpg", "/x/c.jpg"]);
        let full = origin(&["/x/a.jpg", "/x/b.jpg", "/x/c.jpg"]);
        // The driver took the first two and refused the third.
        let sent = snaps.get(..2).unwrap_or_default();
        let done = SendOrigin::new(full.slot.clone(), full.chat_id, full.markup.clone(), sent);
        assert_eq!(
            done.files,
            [PathBuf::from("/x/a.jpg"), PathBuf::from("/x/b.jpg")]
        );
        let tray = [
            PathBuf::from("/x/a.jpg"),
            PathBuf::from("/x/b.jpg"),
            PathBuf::from("/x/c.jpg"),
        ];
        // The tray still holds c.jpg, so the slot is not reset: only the
        // sent files leave it. The caption rode the first message: it goes.
        assert_eq!(
            plan_cleanup(&done, Some("caption"), &tray),
            SentCleanup {
                clear_text: true,
                reset_slot: false
            }
        );
        assert_eq!(
            unsent_text("caption", &snaps, 2),
            "",
            "a caption is not sent twice"
        );
    }

    #[test]
    fn a_partly_sent_long_text_keeps_its_unsent_parts() {
        let parts: Vec<ComposerSnapshot> = ["one", "two", "three"]
            .iter()
            .map(|text| ComposerSnapshot::capture(ChatId(11), ViewGeneration(0), *text))
            .collect();
        let markup = "one\ntwo\nthree";
        assert_eq!(unsent_text(markup, &parts, 1), "two\nthree");
        assert_eq!(unsent_text(markup, &parts, 3), "");
        assert_eq!(unsent_text(markup, &parts, 0), "one\ntwo\nthree");
    }

    #[test]
    fn unsent_text_keeps_the_seam_characters_of_the_original() {
        let parts: Vec<ComposerSnapshot> = ["aaaa", "bbbb", "cccc"]
            .iter()
            .map(|text| ComposerSnapshot::capture(ChatId(11), ViewGeneration(0), *text))
            .collect();
        // A space seam stays a space; a mixed one stays mixed; the markup's
        // own outer whitespace is not part of the text.
        assert_eq!(unsent_text("aaaa bbbb cccc", &parts, 1), "bbbb cccc");
        assert_eq!(
            unsent_text(" aaaa bbbb\n\ncccc ", &parts, 1),
            "bbbb\n\ncccc"
        );
        assert_eq!(unsent_text("aaaa bbbb cccc", &parts, 2), "cccc");
        assert_eq!(unsent_text("aaaa bbbb cccc", &parts, 3), "");
    }

    #[test]
    fn unsent_text_falls_back_to_the_parts_when_the_markup_does_not_match() {
        let parts: Vec<ComposerSnapshot> = ["aaaa", "bbbb"]
            .iter()
            .map(|text| ComposerSnapshot::capture(ChatId(11), ViewGeneration(0), *text))
            .collect();
        assert_eq!(unsent_text("something else", &parts, 1), "bbbb");
    }

    #[test]
    fn the_ack_clear_only_applies_when_the_draft_is_what_was_sent() {
        let sent = origin(&[]);
        let all = plan_cleanup(&sent, Some("caption"), &[]);
        assert!(all.clears_draft_on_ack(""));
        // A partial send kept its unsent text in the draft: the ack must not
        // clear it (the chat may not even be open).
        assert!(!all.clears_draft_on_ack("rest of the text"));
        // Text typed since the send: not what was sent.
        let typed = plan_cleanup(&sent, Some("caption and more"), &[]);
        assert!(!typed.clears_draft_on_ack(""));
        assert!(!plan_cleanup(&sent, None, &[]).clears_draft_on_ack(""));
    }

    /// What `clear_after_send` stores for the ack.
    fn flag_after(sent: &SendOrigin, text_now: &str, left: &str) -> Option<DraftClear> {
        let plan = plan_cleanup(sent, Some(text_now), &[]);
        plan.clears_draft_on_ack(left)
            .then(|| DraftClear::of_text(sent.chat_id, &sent.markup))
    }

    #[test]
    fn a_later_partial_send_cancels_the_flag_of_an_earlier_full_one() {
        let chat = ChatId(11);
        // Send 1 went in full and is not acknowledged yet.
        let mut flag = flag_after(&origin(&[]), "caption", "");
        assert_eq!(flag, Some(DraftClear::of_text(chat, "caption")));
        // Send 2 in the same chat is partial: its leftover stays as the draft.
        let mut second = origin(&[]);
        second.markup = "aaaa bbbb".into();
        flag = flag_after(&second, "aaaa bbbb", "bbbb");
        assert_eq!(flag, None, "the earlier flag is reset");
        // The user switched chats; ack 1 finds no flag and the leftover stays.
        assert!(flag.is_none_or(|flag| !flag.matches(chat, Some("bbbb"))));
    }

    #[test]
    fn the_flag_clears_only_a_draft_still_equal_to_what_was_sent() {
        let chat = ChatId(11);
        let flag = DraftClear::of_text(chat, "caption");
        assert!(flag.matches(chat, Some("caption")));
        assert!(flag.matches(chat, None));
        assert!(!flag.matches(chat, Some("caption and more")));
        assert!(!flag.matches(ChatId(12), Some("caption")));
        assert!(DraftClear::whole_composer(chat).matches(chat, Some("anything")));
    }

    #[test]
    fn nothing_to_compare_clears_no_text() {
        assert!(!plan_cleanup(&origin(&[]), None, &[]).clear_text);
    }
}
