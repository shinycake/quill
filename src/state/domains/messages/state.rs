//! Messages: the menu, reports, links, AI drafts, limits, link previews, scheduling, forwarding, polls, drafts, sponsored rows, translation, export: the `messages` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct MessagesState {
    /// The composer's `@` suggestions for the open chat.
    pub mention_search: Option<MentionSearch>,
    /// What the open message context menu may offer (`messageProperties`).
    pub message_menu_actions:
        Option<(ChatId, MessageId, crate::telegram::envelope::MessageActions)>,
    /// The message menu's Report flow (`reportChat` with message ids).
    pub message_report: Option<MessageReportFlow>,
    /// Viewers, read date and reactors of the message the menu is open on.
    pub message_audience: Option<MessageAudience>,
    /// A reaction chip was right-clicked: its "who reacted" tab opens as
    /// soon as the message's audience is loaded.
    pub wanted_reactor_tab: Option<(ChatId, MessageId, crate::telegram::envelope::ReactionType)>,
    /// One-shot result of an admin moderation call from the delete box
    /// (ban, delete all, report spam); the UI drains it into the status
    /// note.
    pub message_action_note: Option<String>,
    /// M1: parsed `messageLink.link` from the last `getMessageLink` response
    /// (one-shot; the UI copies it to the clipboard and clears it).
    pub message_link_result: Option<String>,
    /// `messageLink.is_public` of that answer: a public link works for
    /// anyone, a private one only for chat members (Telegram Desktop
    /// words the copied-toast differently).
    pub message_link_public: bool,
    /// M1 fix-up: one-shot; set when "Share link" is gated off by
    /// `messageProperties.can_get_link == false` or the `getMessageLink`
    /// request errors. The UI drains it into the status note so the
    /// click never silently does nothing.
    pub message_link_error: Option<String>,
    /// Slice msg-richtext-ai-tools: one-shot `fixTextWithAi` /
    /// `composeTextWithAi` answer for the open chat's composer. The UI
    /// drains it (replacing the draft) on the next frame; the chat id
    /// guards against applying to a chat the user has since left.
    pub ai_composer_text: Option<(ChatId, String)>,
    /// Slice msg-richtext-ai-tools: one-shot `composeRichMessageWithAi`
    /// / `createRichMessageWithAi` / `fixRichMessageWithAi` answer for
    /// the open chat's composer. Same drain contract as
    /// `ai_composer_text`. The UI writes the blocks back as editor markup
    /// (`blocks_to_markup`); the note distinguishes create / fix / rewrite.
    pub ai_composer_blocks: Option<(ChatId, RichMessageContent, &'static str)>,
    /// Slice msg-richtext-ai-tools: one-shot; set when an AI request
    /// errors. The UI drains it into the status note so the click never
    /// silently does nothing.
    pub ai_error: Option<String>,
    /// MED4: `getOption("message_caption_length_max")` via `updateOption`
    /// (TDLib 1.8.67, `schema/td_api.tl:10926`); default 1024 is TDLib's
    /// compiled default. Guards caption edits and media-send captions.
    pub message_caption_length_max: i32,
    /// R8: `getOption("message_text_length_max")` via `updateOption`; 4096 is
    /// the compiled default (Premium raises it). Plain text sends are cut
    /// into several messages at this size (tdesktop `CutPart`); edits over
    /// it are refused.
    pub message_text_length_max: i32,
    /// MED4: one-shot `getWebPageInstantView` answer for the IV reader.
    /// The UI drains it (opens the reader) and clears it.
    pub instant_view: Option<InstantViewPage>,
    /// MED4: one-shot fallback URL when `getWebPageInstantView` errors
    /// (TDLib 404s when the page has no Instant View). The UI drains it
    /// into the browser — TGX behaves the same.
    pub instant_view_fallback_url: Option<String>,
    /// MED4: pending `getWebPageInstantView` URLs by `RequestId`
    /// (`RequestPurpose` stays `Copy`, so the URL rides here).
    pub instant_view_urls: HashMap<RequestId, String>,
    /// MED4b: composer `getLinkPreview` prefetch state — the chip reads
    /// this. Replaced on every new request; cleared when the composer's
    /// detected URL changes away or the composer is submitted.
    pub composer_preview: Option<ComposerLinkPreview>,
    /// MED4b: pending `getLinkPreview` URLs by `RequestId` (same
    /// `Copy`-purpose pattern as `instant_view_urls`).
    pub composer_preview_urls: HashMap<RequestId, String>,
    /// MED2 fix-up: one-shot; set when TDLib refuses a `recognizeSpeech`
    /// request. The UI drains it into the status note — previously the
    /// error fell into the `_ => {}` swallower and the user saw
    /// "transcription requested" followed by silence.
    pub recognize_speech_error: Option<String>,
    /// M1 fix-up: one-shot; set when a `resendMessages` request errors.
    /// The UI drains it into the status note — previously the error fell
    /// into the `_ => {}` swallower and the user saw "retrying send…"
    /// followed by silence.
    pub resend_error: Option<String>,
    pub send_permission_error: Option<String>,
    /// Q1: one-shot; "Too many attempts. Try again in N seconds." after a
    /// user action (send, edit, join, ...) hit a rate limit. The UI drains
    /// it into the status note; the composer text is left untouched.
    pub flood_notice: Option<String>,
    /// M1: `getChatScheduledMessages` results — the chat's scheduled sends,
    /// with `scheduling_state` showing the planned send time.
    pub scheduled_messages: Vec<ParsedMessage>,
    /// Replied-to messages outside the loaded window, keyed by the
    /// replying message `(chat_id, message_id)`.
    pub reply_targets: HashMap<(i64, i64), ReplyTarget>,
    pub pending_bot_messages: HashMap<(i64, i32), PendingBotMessage>,
    pub pending_bot_period_secs: u64,
    /// `poll.id` → `(chat_id, message_id)` of rows loaded with that poll,
    /// so `updatePoll` (which carries no chat or message id) touches only
    /// its rows. Entries can be stale; `apply_update_poll` re-checks and
    /// prunes them.
    pub(crate) poll_messages: HashMap<i64, HashSet<(i64, i64)>>,
    /// In-flight `forwardMessages` (dest / source / requested count).
    pub in_flight_forward: Option<ForwardFlight>,
    /// Further `forwardMessages` in flight while the share box sends to
    /// several chats at once (`in_flight_forward` holds the first).
    pub queued_forward_flights: Vec<ForwardFlight>,
    /// Share box search (local `searchChats` + `searchChatsOnServer`).
    pub share_search: ShareSearch,
    /// Last `forwardMessages` outcome for the dest picker success surface.
    pub last_forward: Option<ForwardResult>,
    /// Each chat's pinned messages, newest first, as last fetched
    /// (`RequestPurpose::GetPinnedMessages`). Absent until fetched; the
    /// pinned bar then falls back to pinned rows in loaded history.
    pub pinned_messages: HashMap<i64, Vec<HistoryMessage>>,
    /// The oldest unread mention/reaction found for the corner buttons;
    /// the driver takes it and jumps (`ConnectDriver::ingest`).
    pub(crate) unread_jump: Option<MessageId>,
    /// B4: `getPollVoters` fetch state for the poll-voters dialog, keyed
    /// by (chat id, message id, 0-based option index). One page per key.
    pub poll_voters: HashMap<(i64, i64, i32), PollVotersFetch>,
    /// B15: `getPollVoteStatistics` fetch state, keyed by (chat id,
    /// message id).
    pub poll_stats: HashMap<(i64, i64), PollStatsFetch>,
    /// Composer text changed since the last persisted draft. Remote
    /// `updateChatDraftMessage` must not replace it (schema comment).
    pub(crate) draft_dirty: HashSet<i64>,
    /// Send succeeded; UI clears the server draft if the composer is still empty.
    pub draft_clears: Vec<ChatId>,
    /// Sponsored messages per chat (`getChatSponsoredMessages`).
    pub sponsored: HashMap<i64, ChatSponsoredMessages>,
    /// Set once Telegram confirmed `reportSponsoredResultAdsHidden`: no ads
    /// are fetched or shown for the rest of this session.
    pub sponsored_hidden: bool,
    /// In-flight sponsored-message report waiting on an option choice.
    pub sponsored_report: Option<SponsoredReportFlight>,
    /// Report target chosen by the user (chat + sponsored message id); cleared
    /// when the flow resolves.
    pub(crate) sponsored_report_target: Option<(ChatId, i64)>,
    /// Last `reportChatSponsoredMessage` outcome note.
    pub last_sponsored_report: Option<SponsoredReportOutcome>,
    /// Translation state (`translateText` / `translateMessageText`, the
    /// chat translate bar).
    pub translate: TranslateState,
    /// `parity:platform-chat-export` — in-progress chat history export.
    /// The driver pages `getChatHistory` into this; the UI surfaces the
    /// result (path or error) and clears it.
    pub chat_export: Option<crate::chat_export::ChatExportState>,
    /// Notification-tone limits (`notification_sound_*_max` options).
    pub tone_limits: crate::message_menu::ToneLimits,
}

impl MessagesState {
    pub(crate) fn new() -> Self {
        Self {
            mention_search: None,
            message_menu_actions: None,
            message_report: None,
            message_audience: None,
            wanted_reactor_tab: None,
            message_action_note: None,
            message_link_result: None,
            message_link_public: false,
            message_link_error: None,
            ai_composer_text: None,
            ai_composer_blocks: None,
            ai_error: None,
            message_caption_length_max: 1024,
            message_text_length_max: 4096,
            instant_view: None,
            instant_view_fallback_url: None,
            instant_view_urls: HashMap::new(),
            composer_preview: None,
            composer_preview_urls: HashMap::new(),
            recognize_speech_error: None,
            resend_error: None,
            send_permission_error: None,
            flood_notice: None,
            scheduled_messages: Vec::new(),
            reply_targets: HashMap::new(),
            pending_bot_messages: HashMap::new(),
            pending_bot_period_secs: 30,
            poll_messages: HashMap::new(),
            in_flight_forward: None,
            queued_forward_flights: Vec::new(),
            share_search: ShareSearch::default(),
            last_forward: None,
            pinned_messages: HashMap::new(),
            unread_jump: None,
            poll_voters: HashMap::new(),
            poll_stats: HashMap::new(),
            draft_dirty: HashSet::new(),
            draft_clears: Vec::new(),
            sponsored: HashMap::new(),
            sponsored_hidden: false,
            sponsored_report: None,
            sponsored_report_target: None,
            last_sponsored_report: None,
            translate: TranslateState::default(),
            chat_export: None,
            tone_limits: crate::message_menu::ToneLimits::default(),
        }
    }
}
