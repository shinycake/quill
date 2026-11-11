//! Sponsored messages of a chat: the fetched list, report flights and
//! outcomes.
use super::*;

/// Sponsored messages for one chat (`getChatSponsoredMessages` response).
/// Rows render with a **Sponsored** / **Recommended** label.
#[derive(Debug, Clone, Default)]
pub struct ChatSponsoredMessages {
    pub messages: Vec<SponsoredMessage>,
    /// Schema `messages_between`: minimum number of ordinary messages between
    /// shown sponsored rows (0 = after all ordinary messages).
    pub messages_between: i32,
    /// Ids already reported to TDLib as viewed (`viewMessages`): an ad is
    /// counted once per fetched list, however often it scrolls into view.
    pub viewed: std::collections::HashSet<i64>,
    /// Ids the user reported or hid; never shown again for this list.
    pub dismissed: std::collections::HashSet<i64>,
    /// When the response arrived (tdesktop `lastReceived`): refetches inside
    /// `SPONSORED_REFETCH_AFTER` are skipped.
    pub fetched_at: Option<std::time::Instant>,
    /// The ads placed in the history so far, in placement order (tdesktop
    /// `Entry::item`): each sits after the message it was placed under and
    /// stays there as newer messages arrive. An ad not listed here is not in
    /// the history yet. Cleared when the chat is left (`clearItems`).
    pub placed: Vec<SponsoredPlacement>,
}

/// tdesktop `kRequestTimeLimit`: a chat's sponsored messages are not
/// requested again for five minutes.
pub const SPONSORED_REFETCH_AFTER: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl ChatSponsoredMessages {
    /// Rows in TDLib's response order. The schema does not promise an order,
    /// so the vector order is preserved verbatim.
    pub fn ordered(&self) -> Vec<&SponsoredMessage> {
        self.messages.iter().collect()
    }
}

/// `reportChatSponsoredMessage` waiting on the user's option choice
/// (`reportSponsoredResultOptionRequired`).
#[derive(Debug, Clone)]
pub struct SponsoredReportFlight {
    pub extra: RequestId,
    pub chat_id: ChatId,
    /// int53 sponsored message id.
    pub message_id: i64,
    pub title: String,
    pub options: Vec<ReportOption>,
}

/// Last `reportChatSponsoredMessage` outcome (no TDLib text is echoed).
#[derive(Debug, Clone)]
pub struct SponsoredReportOutcome {
    pub chat_id: ChatId,
    /// int53 sponsored message id.
    pub message_id: i64,
    pub result: ReportSponsoredResult,
}

/// Slice B2: fetch state of `getBotSimilarBots` for a bot profile. The
/// `users` payload carries only ids (names resolve through
/// `Session::users`); presence records "fetched" so the driver never
/// retries, and `Loading` dedupes the request.
#[derive(Debug, Clone, Default)]
pub enum SimilarBotsFetch {
    #[default]
    Loading,
    Loaded(Vec<i64>),
}

impl SponsoredReportOutcome {
    pub fn user_message(&self) -> &'static str {
        self.result.user_message()
    }
}
