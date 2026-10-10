//! TDLib updates and answers for history, sending, editing, reactions, polls, translation and message menus.
mod parse;

use crate::ids::{ChatId, MessageId};
use crate::telegram::envelope::*;
pub(crate) use parse::parse_messages_payload;

/// Payloads for history, sending, editing, reactions, polls, translation and message menus; wrapped as
/// [`EnvelopePayload::Messages`].
#[derive(Debug, Clone, PartialEq)]
pub enum MessagesPayload {
    UpdatePendingMessage {
        chat_id: ChatId,
        forum_topic_id: i32,
        draft_id: i64,
        can_stop: bool,
        keep_on_stop: bool,
        content: MessageContent,
        files: Vec<ParsedFile>,
    },
    UpdateStopMessageDraft {
        chat_id: ChatId,
        forum_topic_id: i32,
        draft_id: i64,
    },
    UpdateNewMessage(ParsedMessage),
    UpdateMessageSendSucceeded {
        message: ParsedMessage,
        old_message_id: MessageId,
    },
    UpdateMessageSendFailed {
        message: ParsedMessage,
        old_message_id: MessageId,
        error: TdError,
    },
    UpdateMessageSendAcknowledged {
        chat_id: ChatId,
        message_id: MessageId,
    },
    UpdateDeleteMessages {
        chat_id: ChatId,
        message_ids: Vec<MessageId>,
        is_permanent: bool,
        from_cache: bool,
    },
    UpdateMessageContent {
        chat_id: ChatId,
        message_id: MessageId,
        content: MessageContent,
        files: Vec<ParsedFile>,
    },
    /// `updateMessageEphemeralContent` (TDLib 1.8.67,
    /// `schema/td_api.tl:10424`) — the secret-chat ephemeral content of a
    /// message refreshed over time; replaces `message.ephemeral_content`
    /// in place (secret-chat lane, `parity:msg-ephemeral-updates`).
    /// `None` = schema-legal explicit null ("no ephemeral content anymore"),
    /// which clears the stored content; only a missing/mistyped field is a
    /// parse error.
    UpdateMessageEphemeralContent {
        chat_id: ChatId,
        message_id: MessageId,
        ephemeral: Option<EphemeralMessageContent>,
    },
    /// `updateMessageContentOpened` — voice note listened (`is_listened`) or
    /// video note viewed (`is_viewed`).
    UpdateMessageContentOpened {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// `updateMessageEdited` (TDLib 1.8.67, `schema/td_api.tl:10431`): bots
    /// edit inline keyboards this way — the new `reply_markup` replaces the
    /// message's keyboard (Phase 3.2).
    UpdateMessageEdited {
        chat_id: ChatId,
        message_id: MessageId,
        edit_date: i32,
        reply_markup: Option<ReplyMarkup>,
    },
    /// `updatePoll` (TDLib 1.8.67, `schema/td_api.tl:11179`): vote counts /
    /// chosen marks changed. Carries only the new `poll` — no chat or
    /// message id — so the reducer matches it by `poll.id` (Phase 4.2).
    UpdatePoll {
        poll: Poll,
    },
    /// B15: `pollVoteStatistics` (schema 1.8.67, line 10263) — the
    /// `getPollVoteStatistics` answer (line 12947).
    PollVoteStatistics {
        graph: StatisticalGraph,
    },
    /// `messageAutoDeleteTime` — `getDefaultMessageAutoDeleteTime` response
    /// (schema 1.8.67, line 9057).
    MessageAutoDeleteTime {
        seconds: i32,
    },
    /// `updateMessageUnreadReactions` (schema 1.8.67, line 10450): the
    /// chat's new reaction counter plus the newest unread reaction (the
    /// notification source). `newest` is `None` when the list is empty
    /// (a reaction was read).
    UpdateMessageUnreadReactions {
        chat_id: ChatId,
        message_id: MessageId,
        unread_reaction_count: i32,
        newest: Option<UnreadReaction>,
    },
    /// `updateChatDraftMessage`. Positions are the new chat-list orders.
    UpdateChatDraftMessage {
        chat_id: ChatId,
        draft: Option<ChatDraft>,
        positions: Vec<ChatPositionUpdate>,
    },
    /// `updateActiveLiveLocationMessages` (schema line 11386).
    UpdateActiveLiveLocationMessages {
        shares: Vec<ActiveLiveShare>,
    },
    /// `updateMessageLiveLocationViewed` (schema line 10837).
    UpdateMessageLiveLocationViewed {
        chat_id: ChatId,
        message_id: MessageId,
    },
    Messages(Vec<ParsedMessage>),
    Message(ParsedMessage),
    /// M1: `messageLink` (TDLib 1.8.67, `schema/td_api.tl:9666` —
    /// `messageLink link is_public`) — the `getMessageLink` answer. The
    /// driver stashes `link` in `Session::message_link_result`; the UI
    /// copies it to the clipboard.
    MessageLink {
        link: String,
        is_public: bool,
    },
    /// M2: `richMessage` (TDLib 1.8.67, `schema/td_api.tl:5143`) — the
    /// `getFullRichMessage` answer. The driver replaces the blocks of the
    /// partially-received message in history with the full blocks.
    RichMessage {
        rich: RichMessageContent,
    },
    /// MED4: `webPageInstantView` (TDLib 1.8.67, `schema/td_api.tl:4377`)
    /// — the `getWebPageInstantView` answer. `blocks` are the same
    /// `pageBlock*` list as `richMessage`, so the IV reader reuses the M2
    /// block parser/renderer verbatim.
    WebPageInstantView {
        rich: RichMessageContent,
    },
    /// MED4b: `linkPreview` (TDLib 1.8.67, `schema/td_api.tl:4570`) — the
    /// `getLinkPreview` answer for the composer prefetch. `None` when the
    /// payload isn't a well-formed `linkPreview` (defensive; a success
    /// always carries the object).
    LinkPreview {
        preview: Option<LinkPreview>,
    },
    /// M1 fix-up: `messageProperties` (TDLib 1.8.67,
    /// `schema/td_api.tl:11557`) — the `getMessageProperties` answer.
    /// Only `can_get_link` is kept: `getMessageLink` is "available only
    /// if messageProperties.can_get_link" (schema line 12056), so the
    /// driver gates the link request on it instead of letting "Share
    /// link" silently 400.
    MessageProperties(MessageActions),
    /// `messageViewers` (`getMessageViewers` answer).
    MessageViewers(Vec<MessageViewer>),
    /// `MessageReadDate` (`getMessageReadDate` answer).
    MessageReadDate(MessageReadDate),
    /// `addedReactions` (`getMessageAddedReactions` answer).
    AddedReactions(AddedReactionsPage),
    /// B4: `pollVoters` (TDLib 1.8.67, `schema/td_api.tl:2854`) — the
    /// `getPollVoters` answer (schema line 12941). `total_count` is the
    /// approximate total; `voters` is one page of senders, in server
    /// order. Only the senders are kept (the `date` is unused).
    PollVoters {
        total_count: i32,
        voters: Vec<MessageSender>,
    },
    /// `sponsoredMessages` — `getChatSponsoredMessages`. `messages_between` is
    /// the minimum number of ordinary messages between shown sponsored rows
    /// (0 = show after all ordinary messages).
    SponsoredMessages {
        messages: Vec<SponsoredMessage>,
        files: Vec<ParsedFile>,
        messages_between: i32,
    },
    /// `ReportSponsoredResult` — `reportChatSponsoredMessage` /
    /// `reportSponsoredChat` response.
    ReportSponsoredResult(ReportSponsoredResult),
    /// `updateMessageInteractionInfo` — views / forwards / `messageReactions`.
    /// `updateMessageFactCheck`: the fact check of a message changed.
    UpdateMessageFactCheck {
        chat_id: ChatId,
        message_id: MessageId,
        text: String,
    },
    UpdateMessageInteractionInfo {
        chat_id: ChatId,
        message_id: MessageId,
        interaction_info: Option<MessageInteractionInfo>,
    },
    /// `updateMessageIsPinned` — message pin state changed (TDLib 1.8.67).
    UpdateMessageIsPinned {
        chat_id: ChatId,
        message_id: MessageId,
        is_pinned: bool,
    },
    /// `updateChatHasScheduledMessages` — the chat gained its first or lost
    /// its last scheduled message.
    UpdateChatHasScheduledMessages {
        chat_id: i64,
        has_scheduled_messages: bool,
    },
    /// `updateChatMessageSender` (schema 1.8.67, line 10546) — the "send as"
    /// identity of the chat changed.
    UpdateChatMessageSender {
        chat_id: i64,
        message_sender: Option<MessageSender>,
    },
    /// `chatMessageSenders` — answer of `getChatAvailableMessageSenders`.
    ChatMessageSenders {
        senders: Vec<AvailableMessageSender>,
    },
    /// `updateChatIsTranslatable` (schema 1.8.67, line 10585) — translation
    /// of the chat's messages was enabled or disabled.
    UpdateChatIsTranslatable {
        chat_id: i64,
        is_translatable: bool,
    },
}
