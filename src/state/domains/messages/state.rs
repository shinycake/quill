//! Messages: the menu, reports, links, AI drafts, limits, link previews, scheduling, forwarding, polls, drafts, sponsored rows, translation, export: the `messages` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::composer::{DraftKey, DraftTopic};
use crate::state::*;

pub struct MessagesState {
    /// The composer's `@` suggestions for the open chat.
    pub mention_search: Option<MentionSearch>,
    pub rows_on_screen: Option<RowsOnScreen>,
    /// What the open message context menu may offer (`messageProperties`).
    pub menu_actions: MenuActions,
    /// The chips under a message before a reaction request changed them,
    /// put back if TDLib refuses it.
    pub reaction_undo: std::collections::HashMap<
        RequestId,
        (
            ChatId,
            MessageId,
            Option<crate::telegram::envelope::MessageInteractionInfo>,
        ),
    >,
    /// What the open poll-option menu may offer (`pollOptionProperties`).
    pub poll_option_menu: Option<PollOptionMenu>,
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
    pub message_action_note: Notes,
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
    pub message_link_error: Notes,
    /// Slice msg-richtext-ai-tools: one-shot `fixTextWithAi` /
    /// `composeTextWithAi` answer for the open chat's composer. The UI
    /// drains it (replacing the draft) on the next frame. The first id is
    /// the window that asked (`None`: the main window): only that window
    /// takes the answer, even when another shows the same chat; the chat
    /// id guards against applying to a chat the window has since left.
    pub ai_composer_text: Option<(Option<ChatId>, ChatId, String)>,
    /// Slice msg-richtext-ai-tools: one-shot `composeRichMessageWithAi`
    /// / `createRichMessageWithAi` / `fixRichMessageWithAi` answer for
    /// the open chat's composer. Same drain contract as
    /// `ai_composer_text`. The UI writes the blocks back as editor markup
    /// (`blocks_to_markup`); the note distinguishes create / fix / rewrite.
    pub ai_composer_blocks: Option<(Option<ChatId>, ChatId, RichMessageContent, &'static str)>,
    /// Slice msg-richtext-ai-tools: one-shot; set when an AI request
    /// errors. The UI drains it into the status note so the click never
    /// silently does nothing.
    pub ai_error: Notes,
    /// MED4: `getOption("message_caption_length_max")` via `updateOption`
    /// (TDLib 1.8.67, `schema/td_api.tl:10926`); default 1024 is TDLib's
    /// compiled default. Guards caption edits and media-send captions.
    pub message_caption_length_max: i32,
    /// `getOption("paid_media_message_star_count_max")`: the dearest paid
    /// media price in Stars; 10000 until the option arrives (tdesktop's
    /// `stars_paid_post_amount_max` default).
    pub paid_media_star_max: i64,
    /// R8: `getOption("message_text_length_max")` via `updateOption`; 4096 is
    /// the compiled default (Premium raises it). Plain text sends are cut
    /// into several messages at this size (tdesktop `CutPart`); edits over
    /// it are refused.
    pub message_text_length_max: i32,
    /// MED4: one-shot `getWebPageInstantView` answer for the IV reader.
    /// The UI drains it (opens the reader) and clears it.
    pub instant_view: Option<InstantViewPage>,
    /// MED4: links whose `getWebPageInstantView` errored (TDLib 404s when
    /// the page has no Instant View), each for the window that asked. A
    /// window drains its own into the browser (TGX does the same).
    pub instant_view_fallbacks: Vec<InstantViewFallback>,
    /// `getWebPageInstantView` requests sent while the agent UI driver
    /// injected input (`guard::injecting`).
    pub instant_view_agent_requests: HashSet<RequestId>,
    /// MED4: pending `getWebPageInstantView` URLs by `RequestId`
    /// (`RequestPurpose` stays `Copy`, so the URL rides here).
    pub instant_view_urls: HashMap<RequestId, String>,
    /// MED4b: composer `getLinkPreview` prefetch state, by URL: each
    /// window's chip reads the preview of its own composer's URL.
    pub composer_previews: crate::state::ComposerLinkPreviews,
    /// MED4b: pending `getLinkPreview` URLs by `RequestId` (same
    /// `Copy`-purpose pattern as `instant_view_urls`).
    pub composer_preview_urls: HashMap<RequestId, String>,
    /// MED2 fix-up: one-shot; set when TDLib refuses a `recognizeSpeech`
    /// request. The UI drains it into the status note — previously the
    /// error fell into the `_ => {}` swallower and the user saw
    /// "transcription requested" followed by silence.
    pub recognize_speech_error: Notes,
    /// M1 fix-up: one-shot; set when a `resendMessages` request errors.
    /// The UI drains it into the status note — previously the error fell
    /// into the `_ => {}` swallower and the user saw "retrying send…"
    /// followed by silence.
    pub resend_error: Notes,
    pub send_permission_error: Notes,
    /// Q1: one-shot; "Too many attempts. Try again in N seconds." after a
    /// user action (send, edit, join, ...) hit a rate limit. The UI drains
    /// it into the status note; the composer text is left untouched.
    pub flood_notice: Notes,
    /// M1: `getChatScheduledMessages` results — scheduled sends, with
    /// `scheduling_state` showing the planned send time. Rows of every chat
    /// asked for: read one chat's through `Session::scheduled_in`.
    pub scheduled_messages: Vec<ParsedMessage>,
    /// Replied-to messages outside the loaded window, keyed by the
    /// replying message `(chat_id, message_id)`.
    pub reply_targets: HashMap<(i64, i64), ReplyTarget>,
    /// Messages the composer's reply bar points at that were not in memory,
    /// fetched with `getMessage`, keyed by `(chat_id, message_id)`.
    pub composer_reply_fetch: HashMap<(i64, i64), ComposerReplyFetch>,
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
    /// The oldest unread mention/reaction found for the corner buttons,
    /// for the window and chat that asked; the driver takes it and jumps
    /// in that window (`ConnectDriver::ingest`).
    pub(crate) unread_jump: Option<ViewJump>,
    /// B4: `getPollVoters` fetch state for the poll-voters dialog, keyed
    /// by (chat id, message id, 0-based option index). One page per key.
    pub poll_voters: HashMap<(i64, i64, i32), PollVotersFetch>,
    /// B15: `getPollVoteStatistics` fetch state, keyed by (chat id,
    /// message id).
    pub poll_stats: HashMap<(i64, i64), PollStatsFetch>,
    /// Composer text changed since the last persisted draft of that
    /// (chat, topic) slot. Remote `updateChatDraftMessage` /
    /// `updateForumTopic` must not replace it (schema comment).
    pub(crate) draft_dirty: HashSet<DraftKey>,
    /// Counts remote draft changes that replaced a stored draft. A
    /// composer showing that slot compares against it to take the draft
    /// another client typed (tdesktop `HistoryWidget::applyCloudDraft`).
    pub(crate) remote_draft_revision: u64,
    /// Send succeeded in this (chat, topic); UI clears the server draft
    /// if the composer is still empty.
    pub draft_clears: Vec<(ChatId, DraftTopic)>,
    /// Captions chosen for media messages that are still being sent; each
    /// goes out as an edit once the message has its real id.
    pub deferred_captions: Vec<crate::composer::DeferredCaption>,
    /// Sponsored messages per chat (`getChatSponsoredMessages`).
    pub sponsored: HashMap<i64, ChatSponsoredMessages>,
    /// Set once Telegram confirmed `reportSponsoredResultAdsHidden`: no ads
    /// are fetched or shown for the rest of this session.
    pub sponsored_hidden: bool,
    /// In-flight sponsored-message report waiting on an option choice.
    pub sponsored_report: Option<SponsoredReportFlight>,
    /// Report target chosen by the user, with the request its answer is
    /// awaited as; cleared when the flow resolves or is dismissed.
    pub(crate) sponsored_report_target: Option<SponsoredReportTarget>,
    /// Last `reportChatSponsoredMessage` outcome note.
    pub last_sponsored_report: Option<SponsoredReportOutcome>,
    /// Translation state (`translateText` / `translateMessageText`, the
    /// chat translate bar).
    pub translate: TranslateState,
    /// Notification-tone limits (`notification_sound_*_max` options).
    pub tone_limits: crate::message_menu::ToneLimits,
    pub ai_compose: AiComposeState,
}

impl MessagesState {
    pub(crate) fn new() -> Self {
        Self {
            mention_search: None,
            rows_on_screen: None,
            menu_actions: MenuActions::default(),
            reaction_undo: Default::default(),
            poll_option_menu: None,
            message_report: None,
            message_audience: None,
            wanted_reactor_tab: None,
            message_action_note: Notes::default(),
            message_link_result: None,
            message_link_public: false,
            message_link_error: Notes::default(),
            ai_composer_text: None,
            ai_composer_blocks: None,
            ai_error: Notes::default(),
            message_caption_length_max: 1024,
            paid_media_star_max: crate::composer::PAID_MEDIA_MAX_STARS,
            message_text_length_max: 4096,
            instant_view: None,
            instant_view_fallbacks: Vec::new(),
            instant_view_agent_requests: HashSet::new(),
            instant_view_urls: HashMap::new(),
            composer_previews: Default::default(),
            composer_preview_urls: HashMap::new(),
            recognize_speech_error: Notes::default(),
            resend_error: Notes::default(),
            send_permission_error: Notes::default(),
            flood_notice: Notes::default(),
            scheduled_messages: Vec::new(),
            reply_targets: HashMap::new(),
            composer_reply_fetch: HashMap::new(),
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
            remote_draft_revision: 0,
            draft_clears: Vec::new(),
            deferred_captions: Vec::new(),
            sponsored: HashMap::new(),
            sponsored_hidden: false,
            sponsored_report: None,
            sponsored_report_target: None,
            last_sponsored_report: None,
            translate: TranslateState::default(),
            tone_limits: crate::message_menu::ToneLimits::default(),
            ai_compose: AiComposeState::default(),
        }
    }
}

/// The ad being reported and the `reportChatSponsoredMessage` request the
/// flow waits on. An answer for any other request (a report the user
/// dismissed, or an earlier step) is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SponsoredReportTarget {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: i64,
    /// `None` between choosing the target and the request going out.
    pub(crate) request: Option<RequestId>,
}
