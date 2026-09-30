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
        self.sponsored.insert(
            chat_id.0,
            ChatSponsoredMessages {
                messages,
                messages_between,
            },
        );
    }

    /// Sponsored rows for the open chat in TDLib's response order. Empty for
    /// gated chats without a fetch and for chats that never had one.
    pub fn open_sponsored_rows(&self) -> Vec<&SponsoredMessage> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        self.sponsored
            .get(&chat_id.0)
            .map(ChatSponsoredMessages::ordered)
            .unwrap_or_default()
    }

    pub fn sponsored_message(&self, chat_id: ChatId, message_id: i64) -> Option<&SponsoredMessage> {
        self.sponsored
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
        self.sponsored_report = None;
        self.sponsored_report_target = Some((chat_id, message_id));
        self.last_sponsored_report = None;
        Some((chat_id, message_id))
    }

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
            .sponsored_report
            .as_ref()
            .map(|flight| flight.message_id)
            // Fallback for a response that arrives after its picker was
            // dismissed: attribute to the latest report target. If the user
            // starts a second report before the first responds, the first
            // response is attributed to the second row — acceptable: reports
            // are fire-and-forget and the outcome banner is per-chat.
            .or_else(|| self.sponsored_report_target.map(|(_, id)| id))
            .unwrap_or(0);
        match result {
            ReportSponsoredResult::OptionRequired { title, options } => {
                self.sponsored_report = Some(SponsoredReportFlight {
                    extra: pending.id,
                    chat_id,
                    message_id,
                    title,
                    options,
                });
            }
            result => {
                self.sponsored_report = None;
                self.sponsored_report_target = None;
                self.last_sponsored_report = Some(SponsoredReportOutcome {
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
        self.sponsored_report = None;
        self.sponsored_report_target = None;
    }

    pub fn clear_sponsored_report_outcome(&mut self) {
        self.last_sponsored_report = None;
    }
}
