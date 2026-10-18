#![deny(clippy::indexing_slicing)]

use super::*;

pub(super) struct ChromeInputs<'a> {
    pub(super) session: Option<&'a Session>,
    pub(super) chat: Option<&'a ChatSummary>,
    pub(super) sender_name: &'a str,
    pub(super) media_roots: &'a [PathBuf],
    pub(super) wide: bool,
}

#[cfg(test)]
thread_local! {
    static CHROME_BUILT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Row chromes derived so far on this thread (a work counter for tests).
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(in crate::ui) fn chrome_built() -> u64 {
    CHROME_BUILT.with(std::cell::Cell::get)
}

impl ChromeInputs<'_> {
    fn is_group(&self) -> bool {
        matches!(
            self.chat.map(|summary| &summary.kind),
            Some(
                ChatKind::BasicGroup { .. }
                    | ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
            )
        )
    }

    pub(super) fn chrome(&self, message: &HistoryMessage, spot: ChromeSpot) -> RowChrome {
        #[cfg(test)]
        CHROME_BUILT.with(|built| built.set(built.get().wrapping_add(1)));
        let session = self.session;
        let is_group = self.is_group();
        let service = matches!(
            message.content,
            MessageContent::ScreenshotTaken | MessageContent::ChatJoinFromCommunity { .. }
        );
        let accent = match message.sender {
            Some(MessageSender::User { user_id }) => session
                .and_then(|s| s.user(user_id))
                .map(|u| u.accent_color_id),
            // Chats carry no parsed name color: derive one from the id the
            // way Telegram assigns defaults.
            Some(MessageSender::Chat { chat_id }) => Some(chat_id.rem_euclid(7) as i32),
            None => None,
        };
        let name = match message.sender {
            Some(MessageSender::User { user_id }) => session
                .and_then(|s| s.user(user_id))
                .map(|u| u.display_name()),
            Some(MessageSender::Chat { chat_id }) => session
                .and_then(|s| s.chats.get(&chat_id))
                .map(|c| c.title.clone()),
            None => None,
        }
        .unwrap_or_else(|| {
            if message.is_outgoing {
                session
                    .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
                    .map_or_else(|| tr(keys::FROM_YOU).into(), |u| u.display_name())
            } else if is_group {
                tr(keys::Q_CHAT_MEMBER).into()
            } else {
                self.sender_name.to_string()
            }
        });
        let sender =
            (!message.is_outgoing && (service || (is_group && spot.sender_changed))).then(|| {
                SenderLabel {
                    name: name.clone(),
                    accent: accent.filter(|_| is_group),
                    status: session.and_then(|s| s.sender_status(message)),
                }
            });
        let receipt = if message.is_outgoing {
            self.chat
                .map(|summary| summary.outbox_receipt(message))
                .unwrap_or(OutboxReceipt::Sent)
        } else {
            OutboxReceipt::None
        };
        let photo = match message.sender {
            Some(MessageSender::User { user_id }) => {
                session.and_then(|s| s.user_photo_path(user_id))
            }
            Some(MessageSender::Chat { chat_id }) => {
                session.and_then(|s| s.chat_photo_path(ChatId(chat_id)))
            }
            None => None,
        }
        .and_then(|path| sandboxed_display_path(path, self.media_roots));
        let sender_avatar = self
            .chat
            .is_some_and(|chat| {
                quill::chat_wide::shows_sender_photo(&chat.kind, message.is_outgoing, self.wide)
            })
            .then(|| {
                if spot.next_same_sender {
                    (String::new(), None)
                } else {
                    (name, photo)
                }
            });
        let headers = spot.with_headers.then_some(session).flatten();
        RowChrome {
            sender,
            receipt,
            sender_avatar,
            reply_header: headers
                .and_then(|s| s.reply_header(message))
                .filter(|_| message.extras.suggested_post.is_none()),
            forward_header: headers.and_then(|s| s.forward_header(message)),
            via_bot: headers.and_then(|s| s.via_bot_label(message)),
        }
    }

    /// Each patched row's new messages and its chrome derived again
    /// (`patch` sorted by row: an album's items share one).
    pub(super) fn row_patches(
        &self,
        rows: &[HistoryRow],
        mut patch: Vec<(usize, HistoryMessage)>,
    ) -> Vec<RowPatch> {
        patch.sort_by_key(|(ix, _)| *ix);
        let mut patches: Vec<RowPatch> = Vec::new();
        for (ix, message) in patch {
            match patches.last_mut() {
                Some(last) if last.ix == ix => last.messages.push(message),
                _ => patches.push(RowPatch {
                    ix,
                    messages: vec![message],
                    chrome: None,
                }),
            }
        }
        for patch in &mut patches {
            let Some(row) = rows.get(patch.ix) else {
                continue;
            };
            let first = row
                .first_id()
                .and_then(|id| patch.messages.iter().find(|message| message.id == id))
                .or_else(|| row.first_message());
            patch.chrome = first.map(|first| self.chrome(first, row.spot()));
        }
        patches
    }

    /// The rows at `due` whose chrome predates `epoch`, with the chrome
    /// derived again (`None` when it came out the same). Patched rows are
    /// skipped: their chrome is new already.
    pub(super) fn stale_rows(
        &self,
        rows: &[HistoryRow],
        due: &[usize],
        patches: &[RowPatch],
        epoch: u64,
    ) -> Vec<(usize, Option<RowChrome>)> {
        due.iter()
            .filter(|ix| !patches.iter().any(|patch| patch.ix == **ix))
            .filter_map(|&ix| {
                let row = rows.get(ix).filter(|row| row.chrome_stamp() != epoch)?;
                let fresh = self.chrome(row.first_message()?, row.spot());
                Some((ix, (fresh != *row.chrome()).then_some(fresh)))
            })
            .collect()
    }
}

/// Rows on screen at the last paint and this many on each side get their
/// chrome derived again after a session change; the others when they are
/// drawn (`render_history_row` marks them due).
const CHROME_WINDOW_MARGIN: usize = 16;

/// New messages for one row (`RowsInput::Patch`), and its chrome.
pub(super) struct RowPatch {
    ix: usize,
    messages: Vec<HistoryMessage>,
    chrome: Option<RowChrome>,
}

/// The chrome generation the rows are brought to this render.
#[derive(Clone, Copy)]
pub(super) struct ChromeMark {
    pub(super) epoch: u64,
    /// `Session::revision_without_file_progress` it stands for.
    pub(super) revision: Option<u64>,
}

impl QuillApp {
    /// (ready-path revision, files asked for): a change remeasures the rows
    /// (media grew in place).
    pub(super) fn history_media_signature(&self) -> (u64, usize) {
        self.session()
            .map(|s| {
                (
                    s.media.files.paths_revision(),
                    s.media.downloads.requested_count(),
                )
            })
            .unwrap_or_default()
    }

    /// This render's chrome generation: a new one when the session changed
    /// in a way the chrome may show, or every render when the session's
    /// revision does not cover every change (screenshot demos).
    pub(super) fn chrome_mark(&self) -> ChromeMark {
        let revision = self.session().map(Session::revision_without_file_progress);
        let epoch = if self.chrome_follows_revision() && self.history.chrome_seen == revision {
            self.history.chrome_epoch
        } else {
            self.history.chrome_epoch.wrapping_add(1)
        };
        ChromeMark { epoch, revision }
    }

    /// Rows whose chrome is brought up to date this render: those around
    /// the last painted ones and those drawn while stale, in order.
    pub(super) fn chrome_due_rows(&self) -> Vec<usize> {
        let len = self.history.rows.len();
        let mut due: Vec<usize> = match self.history.scroll_view_probe.get() {
            Some((first, last)) => (first.saturating_sub(CHROME_WINDOW_MARGIN)
                ..last.saturating_add(CHROME_WINDOW_MARGIN + 1).min(len))
                .collect(),
            None => (0..len).collect(),
        };
        if let Ok(mut drawn) = self.history.chrome_due.try_borrow_mut() {
            due.append(&mut drawn);
        }
        due.sort_unstable();
        due.dedup();
        due
    }

    /// A row drawn with chrome older than the rows around the viewport:
    /// render once more so `chrome_due_rows` brings it up to date. The
    /// list draws rows while the window draws, when a notify is not heard,
    /// so it is sent after the frame (once for all the rows it drew).
    pub(super) fn note_stale_chrome(&self, ix: usize, row: &HistoryRow, cx: &mut Context<Self>) {
        if row.chrome_stamp() == self.history.chrome_epoch {
            return;
        }
        let Ok(mut due) = self.history.chrome_due.try_borrow_mut() else {
            return;
        };
        if due.contains(&ix) {
            return;
        }
        // Keyed on its own flag: a render that skips the kept-rows branch
        // (the chat is hidden) leaves `due` filled, and the next row must
        // still get its notify.
        let first = !self.history.chrome_notify_pending.replace(true);
        due.push(ix);
        drop(due);
        if first {
            let app = cx.weak_entity();
            // The flag lives in this window's history: reset it in the
            // window that set it, not whichever one is in place later.
            let scope = self.windows.scope;
            cx.defer(move |cx| {
                let _ = app.update(cx, |this, cx| this.finish_chrome_notify(scope, cx));
            });
        }
    }

    /// Clear `scope`'s pending-notify flag and notify. When the window
    /// cannot be scoped in (it is live but its session view is gone), the
    /// flag is cleared on its slot directly so chrome notifies cannot stop.
    pub(in crate::ui) fn finish_chrome_notify(
        &mut self,
        scope: Option<ChatId>,
        cx: &mut Context<Self>,
    ) {
        let scoped = self.in_window_scope(scope, cx, |this, cx| {
            this.history.chrome_notify_pending.set(false);
            cx.notify();
        });
        if scoped.is_some() {
            return;
        }
        if let Some(chat_id) = scope {
            self.clear_slot_chrome_notify(chat_id);
        }
    }

    /// Apply this render's patches and chrome to the kept rows and
    /// remeasure the rows that changed (every row when a file became ready).
    pub(super) fn apply_row_patches(
        &mut self,
        patches: Vec<RowPatch>,
        fresh: Vec<(usize, Option<RowChrome>)>,
        mark: ChromeMark,
        started: std::time::Instant,
        cx: &mut Context<Self>,
    ) {
        self.history.chrome_epoch = mark.epoch;
        self.history.chrome_seen = mark.revision;
        let media_signature = self.history_media_signature();
        let media_changed = media_signature != self.history.media_signature;
        self.history.media_signature = media_signature;
        let checked = patches.len() + fresh.len();
        let patched_messages = !patches.is_empty();
        let mut changed = Vec::with_capacity(checked);
        for patch in patches {
            let Some(row) = self.history.rows.get_mut(patch.ix) else {
                continue;
            };
            self.history.row_stats.messages_cloned += patch.messages.len() as u64;
            for message in patch.messages {
                if let Some(slot) = row.message_mut(message.id) {
                    *slot = message;
                }
            }
            if let Some(chrome) = patch.chrome {
                row.set_chrome(chrome);
            }
            row.set_chrome_stamp(mark.epoch);
            changed.push(patch.ix);
        }
        for (ix, chrome) in fresh {
            let Some(row) = self.history.rows.get_mut(ix) else {
                continue;
            };
            row.set_chrome_stamp(mark.epoch);
            if let Some(chrome) = chrome {
                row.set_chrome(chrome);
                changed.push(ix);
            }
        }
        if checked > 0 {
            self.history.row_stats.note_patched(changed.len(), started);
        }
        if media_changed {
            self.history
                .scroller
                .update(cx, |state, cx| state.remeasure(cx));
        } else if !changed.is_empty() {
            self.history.scroller.update(cx, |state, cx| {
                for ix in changed {
                    let _ = state.remeasure_items(ix..ix + 1, cx);
                }
            });
        }
        if patched_messages {
            self.history_a11y_rows_changed(cx);
        }
    }
}
