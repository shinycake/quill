#![deny(clippy::indexing_slicing)]

use super::*;

pub(super) struct ChromeInputs<'a> {
    pub(super) session: Option<&'a Session>,
    pub(super) chat: Option<&'a ChatSummary>,
    pub(super) sender_name: &'a str,
    pub(super) media_roots: &'a [PathBuf],
    pub(super) wide: bool,
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

    pub(super) fn stale_rows(&self, rows: &[HistoryRow]) -> Vec<(usize, RowChrome)> {
        rows.iter()
            .enumerate()
            .filter_map(|(ix, row)| {
                let fresh = self.chrome(row.first_message()?, row.spot());
                (fresh != *row.chrome()).then_some((ix, fresh))
            })
            .collect()
    }
}

impl QuillApp {
    pub(super) fn history_media_signature(&self) -> (usize, usize) {
        self.session()
            .map(|s| {
                (
                    s.media
                        .files
                        .values()
                        .filter(|file| file.usable_path().is_some())
                        .count(),
                    s.media.downloads.requested_count(),
                )
            })
            .unwrap_or_default()
    }

    pub(super) fn patch_row_chrome(
        &mut self,
        fresh: Vec<(usize, RowChrome)>,
        session_revision: Option<u64>,
        started: std::time::Instant,
        cx: &mut Context<Self>,
    ) {
        self.history.chrome_session_revision = session_revision;
        let media_signature = self.history_media_signature();
        let media_changed = media_signature != self.history.media_signature;
        self.history.media_signature = media_signature;
        let mut patched = Vec::with_capacity(fresh.len());
        for (ix, chrome) in fresh {
            if let Some(row) = self.history.rows.get_mut(ix) {
                row.set_chrome(chrome);
                patched.push(ix);
            }
        }
        self.history.row_stats.note_patched(patched.len(), started);
        if media_changed {
            self.history
                .scroller
                .update(cx, |state, cx| state.remeasure(cx));
        } else if !patched.is_empty() {
            self.history.scroller.update(cx, |state, cx| {
                for ix in patched {
                    let _ = state.remeasure_items(ix..ix + 1, cx);
                }
            });
        }
    }
}
