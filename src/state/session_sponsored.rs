//! Sponsored messages.
use super::*;

impl Session {
    /// Record a `joinChat` outcome. `Success` flips status optimistically;
    /// `updateChatMember` confirms. The other variants keep the old status and
    /// are logged (the UI shows a fixed note).
    pub fn accept_join_chat_result(&mut self, chat_id: ChatId, result: ChatJoinResult) {
        match result {
            ChatJoinResult::Success { .. } => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.set_member_status(ChannelMemberStatus::Member, None);
                }
            }
            other => {
                self.diagnostics.record(Diagnostic {
                    category: "reducer",
                    type_name: Some("joinChat".into()),
                    extra: Some(chat_id.0 as u64),
                    seq: None,
                    note: match other {
                        ChatJoinResult::RequestSent => "join-request-sent",
                        ChatJoinResult::GuardBotApprovalRequired => "join-guard-bot-approval",
                        ChatJoinResult::Declined => "join-declined",
                        ChatJoinResult::Success { .. } => "join-success",
                    },
                });
            }
        }
    }

    /// Store `sponsoredMessages` for a chat (TDLib display order kept; files
    /// remembered for the download sandbox).
    pub fn accept_sponsored_messages(
        &mut self,
        chat_id: ChatId,
        messages: Vec<SponsoredMessage>,
        messages_between: i32,
        files: &[ParsedFile],
    ) {
        self.remember_files(files);
        // A refetch keeps what the user already did with an ad that is
        // still in the list (viewed once, reported/hidden).
        let previous = self
            .messages
            .sponsored
            .remove(&chat_id.0)
            .unwrap_or_default();
        let keep = |ids: &std::collections::HashSet<i64>| -> std::collections::HashSet<i64> {
            ids.iter()
                .copied()
                .filter(|id| messages.iter().any(|m| m.message_id == *id))
                .collect()
        };
        let (viewed, dismissed) = (keep(&previous.viewed), keep(&previous.dismissed));
        self.messages.sponsored.insert(
            chat_id.0,
            ChatSponsoredMessages {
                messages,
                messages_between,
                viewed,
                dismissed,
                fetched_at: Some(std::time::Instant::now()),
            },
        );
    }

    /// Whether `getChatSponsoredMessages` should be sent for `chat_id`: not
    /// after the user hid ads, and not again within five minutes of the last
    /// response (tdesktop `TooEarlyForRequest`).
    pub fn sponsored_fetch_due(&self, chat_id: ChatId, now: std::time::Instant) -> bool {
        if self.messages.sponsored_hidden {
            return false;
        }
        match self
            .messages
            .sponsored
            .get(&chat_id.0)
            .and_then(|entry| entry.fetched_at)
        {
            Some(at) => now.saturating_duration_since(at) >= SPONSORED_REFETCH_AFTER,
            None => true,
        }
    }

    /// The ad shown after the last message of the open chat, as tdesktop
    /// does: the first not-dismissed message of the fetched list, once its
    /// media is downloaded (TDLib: content "must be fully downloaded before
    /// the message is shown"). `None` while the history window stops short of
    /// the latest message, in topic views, and after ads were hidden. The
    /// caller adds the "scrolled to the bottom" condition (a UI fact).
    pub fn open_sponsored_tail(&self) -> Option<&SponsoredMessage> {
        if self.messages.sponsored_hidden || self.open_topic.is_some() {
            return None;
        }
        let chat_id = self.open_chat?;
        if self
            .histories
            .get(&chat_id.0)
            .is_some_and(|history| history.has_newer)
        {
            return None;
        }
        let entry = self.messages.sponsored.get(&chat_id.0)?;
        entry
            .messages
            .iter()
            .find(|message| !entry.dismissed.contains(&message.message_id))
            .filter(|message| {
                message.content_file_ids().iter().all(|id| {
                    self.media
                        .files
                        .get(&id.0)
                        .is_some_and(|file| file.usable_path().is_some())
                })
            })
    }

    /// The UI shows these ads on screen (the whole text, button excluded).
    /// Returns the ids not yet counted, marking them viewed so a scroll away
    /// and back never sends a second `viewMessages`. Ids that are not in the
    /// fetched list of an open chat are ignored.
    pub fn take_sponsored_views(&mut self, chat_id: ChatId, shown: &[i64]) -> Vec<i64> {
        if self.open_chat != Some(chat_id) {
            return Vec::new();
        }
        let Some(entry) = self.messages.sponsored.get_mut(&chat_id.0) else {
            return Vec::new();
        };
        let mut due = Vec::new();
        for id in shown {
            if entry.messages.iter().any(|m| m.message_id == *id) && entry.viewed.insert(*id) {
                due.push(*id);
            }
        }
        due
    }

    /// A `viewMessages` send failed: let the next frame retry these ids.
    pub fn untake_sponsored_views(&mut self, chat_id: ChatId, ids: &[i64]) {
        if let Some(entry) = self.messages.sponsored.get_mut(&chat_id.0) {
            for id in ids {
                entry.viewed.remove(id);
            }
        }
    }

    /// The user chose "Hide ads" on a shown ad. Hiding is the Premium
    /// `toggleHasSponsoredMessagesEnabled(false)` setting (tdesktop's
    /// `HideSponsoredClickHandler`). `None`: the ad is gone. `Some(true)`:
    /// Premium, the caller sends the request. `Some(false)`: not Premium,
    /// the "needs Telegram Premium" notice is recorded and nothing is sent.
    pub fn begin_sponsored_hide(&mut self, chat_id: ChatId, message_id: i64) -> Option<bool> {
        self.sponsored_message(chat_id, message_id)?;
        self.messages.sponsored_report = None;
        self.messages.sponsored_report_target = None;
        if self.my_is_premium() {
            self.messages.last_sponsored_report = None;
            return Some(true);
        }
        self.messages.last_sponsored_report = Some(SponsoredReportOutcome {
            chat_id,
            message_id,
            result: ReportSponsoredResult::PremiumRequired,
        });
        Some(false)
    }

    /// `toggleHasSponsoredMessagesEnabled(false)` answered `ok`: no ads are
    /// shown or fetched for the rest of the session.
    pub fn accept_sponsored_hidden(&mut self, chat_id: ChatId) {
        self.messages.sponsored_hidden = true;
        self.messages.last_sponsored_report = Some(SponsoredReportOutcome {
            chat_id,
            message_id: 0,
            result: ReportSponsoredResult::AdsHidden,
        });
    }

    /// Sponsored rows for the open chat in TDLib's response order. Empty for
    /// gated chats without a fetch and for chats that never had one.
    pub fn open_sponsored_rows(&self) -> Vec<&SponsoredMessage> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        self.messages
            .sponsored
            .get(&chat_id.0)
            .map(ChatSponsoredMessages::ordered)
            .unwrap_or_default()
    }

    pub fn sponsored_message(&self, chat_id: ChatId, message_id: i64) -> Option<&SponsoredMessage> {
        self.messages
            .sponsored
            .get(&chat_id.0)
            .and_then(|entry| entry.messages.iter().find(|m| m.message_id == message_id))
    }

    /// Begin a `reportChatSponsoredMessage` flow. Returns the request
    /// identifiers when the row exists and `can_be_reported` is set.
    pub fn begin_sponsored_report(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
    ) -> Option<(ChatId, i64)> {
        let reportable = self
            .sponsored_message(chat_id, message_id)
            .is_some_and(|message| message.can_be_reported);
        if !reportable {
            return None;
        }
        self.messages.sponsored_report = None;
        self.messages.sponsored_report_target = Some((chat_id, message_id));
        self.messages.last_sponsored_report = None;
        Some((chat_id, message_id))
    }

    /// Apply a `ReportSponsoredResult` for a finished `ReportChatSponsoredMessage`.
    /// Apply a `ReportSponsoredResult` for a finished `ReportChatSponsoredMessage`.
    /// `OptionRequired` arms the option picker; any other result closes it.
    pub fn accept_sponsored_report(
        &mut self,
        pending: &PendingRequest,
        result: ReportSponsoredResult,
    ) {
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let message_id = self
            .messages
            .sponsored_report
            .as_ref()
            .map(|flight| flight.message_id)
            // Fallback for a response that arrives after its picker was
            // dismissed: attribute to the latest report target. If the user
            // starts a second report before the first responds, the first
            // response is attributed to the second row — acceptable: reports
            // are fire-and-forget and the outcome banner is per-chat.
            .or_else(|| self.messages.sponsored_report_target.map(|(_, id)| id))
            .unwrap_or(0);
        match result {
            ReportSponsoredResult::OptionRequired { title, options } => {
                self.messages.sponsored_report = Some(SponsoredReportFlight {
                    extra: pending.id,
                    chat_id,
                    message_id,
                    title,
                    options,
                });
            }
            result => {
                self.messages.sponsored_report = None;
                self.messages.sponsored_report_target = None;
                match &result {
                    // The reported ad leaves the history (tdesktop removes it).
                    ReportSponsoredResult::Ok => {
                        if let Some(entry) = self.messages.sponsored.get_mut(&chat_id.0) {
                            entry.dismissed.insert(message_id);
                        }
                    }
                    ReportSponsoredResult::AdsHidden => self.messages.sponsored_hidden = true,
                    _ => {}
                }
                self.messages.last_sponsored_report = Some(SponsoredReportOutcome {
                    chat_id,
                    message_id,
                    result,
                });
            }
        }
    }

    /// Drop the report picker flight and its target. Called when the user
    /// cancels, when a send fails, or when a result is applied elsewhere —
    /// a dismissed report must not attribute a late response to a stale row.
    pub fn dismiss_sponsored_report(&mut self) {
        self.messages.sponsored_report = None;
        self.messages.sponsored_report_target = None;
    }

    pub fn clear_sponsored_report_outcome(&mut self) {
        self.messages.last_sponsored_report = None;
    }
}
