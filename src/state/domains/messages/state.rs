//! Messages: the menu, reports, links, AI drafts, limits, link previews, scheduling, forwarding, polls, drafts, sponsored rows, translation, export: the `messages` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::composer::{DraftKey, DraftTopic};
use crate::state::*;

/// One optimistic reaction change, kept until TDLib answers its request.
/// A refusal puts `before` back only while the chips still show `after`;
/// once another change landed on top, it takes back just this one.
#[derive(Debug, Clone)]
pub struct ReactionUndo {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub before: Option<crate::telegram::envelope::MessageInteractionInfo>,
    pub after: Option<crate::telegram::envelope::MessageInteractionInfo>,
    pub reaction: crate::telegram::envelope::ReactionType,
    pub add: bool,
}

/// A `messageLink` answer. `is_public`: a public link works for anyone, a
/// private one only for chat members (Telegram Desktop words the
/// copied-toast differently).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageLinkAnswer {
    pub link: String,
    pub is_public: bool,
}

/// How a send of the composer ended, named by the temporary id TDLib gave
/// the message (`old_message_id` of `updateMessageSendSucceeded` and
/// `updateMessageSendFailed`) and the draft slot it went to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendAck {
    pub chat_id: ChatId,
    pub slot: DraftTopic,
    pub old_message_id: MessageId,
}

/// The answer to a send: the temporary message TDLib made for `request`.
/// The composer's pending draft clear holds the requests of its send, so
/// this is how an ack finds the clear of the send it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendAnswered {
    pub request: RequestId,
    pub chat_id: ChatId,
    pub message_id: MessageId,
}

pub struct MessagesState {
    /// The composer's `@` suggestions for the open chat.
    pub mention_search: Option<MentionSearch>,
    pub rows_on_screen: Option<RowsOnScreen>,
    /// What the open message context menu may offer (`messageProperties`).
    pub menu_actions: MenuActions,
    /// The chips under a message before a reaction request changed them,
    /// put back if TDLib refuses it.
    pub reaction_undo: std::collections::HashMap<RequestId, ReactionUndo>,
    /// What the open poll-option menu may offer (`pollOptionProperties`).
    pub poll_option_menu: Option<PollOptionMenu>,
    /// The message menu's Report flow (`reportChat` with message ids).
    pub message_report: Option<MessageReportFlow>,
    /// Viewers, read date and reactors of the message the menu is open on.
    pub message_audience: Option<MessageAudience>,
    /// One-shot result of an admin moderation call from the delete box
    /// (ban, delete all, report spam); the UI drains it into the status
    /// note.
    pub message_action_note: Notes,
    /// M1: parsed `messageLink` from the last `getMessageLink` response
    /// (the link and whether it is public, as one value; the UI copies it
    /// to the clipboard and takes it, so it is read once).
    pub message_link: Option<MessageLinkAnswer>,
    /// M1 fix-up: one-shot; set when "Share link" is gated off by
    /// `messageProperties.can_get_link == false` or the `getMessageLink`
    /// request errors. The UI drains it into the status note so the
    /// click never silently does nothing.
    pub message_link_error: Notes,
    /// Slice msg-richtext-ai-tools: answers of `fixTextWithAi` /
    /// `composeTextWithAi` / the rich-message AI methods (and their
    /// failures) for the composers. Each window's frame takes the answers
    /// of its own asks and applies one only while it holds that request
    /// (`AiAsk::Asked`); a queue, so two answers in one frame are both
    /// kept.
    pub ai_answers: Vec<AiAnswer>,
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
    /// MED4: pending `getWebPageInstantView` requests by `RequestId`, with
    /// the URL asked and whether an agent's step sent it
    /// ([`InstantViewAsk`]).
    pub instant_view_requests: HashMap<RequestId, InstantViewAsk>,
    /// MED4b: composer `getLinkPreview` prefetch state, by URL: each
    /// window's chip reads the preview of its own composer's URL.
    pub composer_previews: crate::state::ComposerLinkPreviews,
    /// MED2 fix-up: one-shot; set when TDLib refuses a `recognizeSpeech`
    /// request. The UI drains it into the status note — previously the
    /// error fell into the `_ => {}` swallower and the user saw
    /// "transcription requested" followed by silence.
    pub recognize_speech_error: Notes,
    /// "Load full message" of a partial rich message, per message: loading
    /// from the send until the answer (which removes the entry, the message
    /// is full now) or the error (`Failed`, tap again to retry).
    pub full_rich_loads: HashMap<crate::ids::ChatMessage, crate::state::fetch::Fetch<()>>,
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
    /// M1: `getChatScheduledMessages` results and the asks out for them.
    pub scheduled_messages: ScheduledMessages,
    /// Replied-to messages outside the loaded window, keyed by the
    /// replying message `(chat_id, message_id)`.
    pub reply_targets: HashMap<(i64, i64), ReplyTarget>,
    /// Messages the composer's reply bar points at that were not in memory,
    /// fetched with `getMessage`, keyed by `(chat_id, message_id)`.
    pub composer_reply_fetch: ReplyFetches,
    pub pending_bot_messages: HashMap<(i64, i32), PendingBotMessage>,
    pub pending_bot_period_secs: u64,
    /// `poll.id` → `(chat_id, message_id)` of rows loaded with that poll,
    /// so `updatePoll` (which carries no chat or message id) touches only
    /// its rows. Entries can be stale; `apply_update_poll` re-checks and
    /// prunes them.
    pub(crate) poll_messages: HashMap<i64, HashSet<(i64, i64)>>,
    /// In-flight `forwardMessages` (dest / source / requested count) and
    /// the outcomes the dest picker's success surface has not shown yet.
    pub forwards: Forwards,
    /// Share box search (local `searchChats` + `searchChatsOnServer`).
    pub share_search: ShareSearch,
    /// Each chat's pinned messages, newest first, as last fetched
    /// (`RequestPurpose::GetPinnedMessages`). Absent or not answered yet,
    /// the pinned bar falls back to pinned rows in loaded history; a failed
    /// fetch stays `Failed` and is not asked again by the next pull, only by
    /// the chat opening.
    pub pinned_messages: HashMap<i64, crate::state::fetch::Fetch<Vec<HistoryMessage>>>,
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
    pub draft_clears: Vec<SendAck>,
    /// Send failed in this (chat, topic): a draft clear that waited on its
    /// ack is dropped, the draft stays.
    pub failed_sends: Vec<SendAck>,
    /// Sends of the composer answered since the UI last looked: the
    /// temporary message each request made (`SendMessage` answers).
    pub sent_answers: Vec<SendAnswered>,
    /// Captions chosen for media messages that are still being sent; each
    /// goes out as an edit once the message has its real id.
    pub deferred_captions: Vec<crate::composer::DeferredCaption>,
    /// Sponsored messages per chat (`getChatSponsoredMessages`).
    pub sponsored: HashMap<i64, ChatSponsoredMessages>,
    /// The sponsored-message report flow: its target and the request
    /// awaited, the option picker, or the last outcome note
    /// ([`SponsoredReportFlow`]).
    pub sponsored_report: SponsoredReportFlow,
    /// Translation state (`translateText` / `translateMessageText`, the
    /// chat translate bar).
    pub translate: TranslateState,
    /// Notification-tone limits (`notification_sound_*_max` options).
    pub tone_limits: crate::message_menu::ToneLimits,
    pub ai_compose: AiComposeState,
}

impl MessagesState {
    /// The fetched pinned messages of a chat, when an answer is held.
    pub fn pinned_data(&self, chat_id: i64) -> Option<&Vec<HistoryMessage>> {
        self.pinned_messages.get(&chat_id)?.data()
    }

    /// A confirmed unpin-all left the chat with no pinned message. That is
    /// the new truth whichever fetch was in flight: a late answer of an
    /// older one finds the list `Ready` and is dropped (`Fetch::answered`
    /// takes only a waiting fetch).
    pub(crate) fn pins_cleared(&mut self, chat_id: i64) {
        self.pinned_messages
            .entry(chat_id)
            .or_default()
            .pushed(Vec::new());
    }

    pub(crate) fn new() -> Self {
        Self {
            mention_search: None,
            rows_on_screen: None,
            menu_actions: MenuActions::default(),
            reaction_undo: Default::default(),
            poll_option_menu: None,
            message_report: None,
            message_audience: None,
            message_action_note: Notes::default(),
            message_link: None,
            message_link_error: Notes::default(),
            ai_answers: Vec::new(),
            ai_error: Notes::default(),
            message_caption_length_max: 1024,
            paid_media_star_max: crate::composer::PAID_MEDIA_MAX_STARS,
            message_text_length_max: 4096,
            instant_view: None,
            instant_view_fallbacks: Vec::new(),
            instant_view_requests: HashMap::new(),
            composer_previews: Default::default(),
            recognize_speech_error: Notes::default(),
            full_rich_loads: HashMap::new(),
            resend_error: Notes::default(),
            send_permission_error: Notes::default(),
            flood_notice: Notes::default(),
            scheduled_messages: ScheduledMessages::default(),
            reply_targets: HashMap::new(),
            composer_reply_fetch: Default::default(),
            pending_bot_messages: HashMap::new(),
            pending_bot_period_secs: 30,
            poll_messages: HashMap::new(),
            forwards: Forwards::default(),
            share_search: ShareSearch::default(),
            pinned_messages: HashMap::new(),
            poll_voters: HashMap::new(),
            poll_stats: HashMap::new(),
            draft_dirty: HashSet::new(),
            remote_draft_revision: 0,
            draft_clears: Vec::new(),
            failed_sends: Vec::new(),
            sent_answers: Vec::new(),
            deferred_captions: Vec::new(),
            sponsored: HashMap::new(),
            sponsored_report: SponsoredReportFlow::Idle,
            translate: TranslateState::default(),
            tone_limits: crate::message_menu::ToneLimits::default(),
            ai_compose: AiComposeState::default(),
        }
    }
}

/// The ad being reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SponsoredReportTarget {
    pub chat_id: ChatId,
    pub message_id: i64,
}

/// The `reportChatSponsoredMessage` flow, one stage at a time.
///
/// `Choosing` is the target picked before the request is sent; it leaves
/// when the send succeeds (`Asking`) or fails (`dismiss`, `Idle`). `Asking`
/// leaves on the answer of its request (`Picking` when TDLib wants an
/// option, `Done` otherwise), on the request's error (`Idle`), or on Cancel
/// (`Idle`). `Picking` holds no request: choosing an option starts a new
/// flow (`Choosing`), Cancel leaves it. `Done` leaves when the UI dismisses
/// the note or a new report starts. An answer for any other request is
/// dropped.
#[derive(Debug, Clone, Default)]
pub enum SponsoredReportFlow {
    #[default]
    Idle,
    /// The ad being reported; the request has not gone out yet.
    Choosing(SponsoredReportTarget),
    /// The ad being reported and the request its answer is awaited as.
    Asking {
        target: SponsoredReportTarget,
        request: RequestId,
    },
    /// TDLib wants an option; nothing is in flight.
    Picking(SponsoredReportFlight),
    /// The last outcome note (also `AdsHidden` from "Hide ads").
    Done(SponsoredReportOutcome),
}

impl SponsoredReportFlow {
    /// The report waiting on an option choice.
    pub fn flight(&self) -> Option<&SponsoredReportFlight> {
        match self {
            Self::Picking(flight) => Some(flight),
            _ => None,
        }
    }

    /// The outcome note of the last report.
    pub fn outcome(&self) -> Option<&SponsoredReportOutcome> {
        match self {
            Self::Done(outcome) => Some(outcome),
            _ => None,
        }
    }

    pub fn target(&self) -> Option<SponsoredReportTarget> {
        match self {
            Self::Choosing(target) | Self::Asking { target, .. } => Some(*target),
            Self::Picking(flight) => Some(SponsoredReportTarget {
                chat_id: flight.chat_id,
                message_id: flight.message_id,
            }),
            Self::Idle | Self::Done(_) => None,
        }
    }

    /// The request the flow waits on, if one is out.
    pub fn awaited(&self) -> Option<RequestId> {
        match self {
            Self::Asking { request, .. } => Some(*request),
            _ => None,
        }
    }

    /// The report picker or the request goes; an outcome note stays.
    pub fn dismiss(&mut self) {
        if self.target().is_some() {
            *self = Self::Idle;
        }
    }
}
