//! State behind the message context menu's "Report" flow and its
//! "N Seen" / "N Reacted" lists (Telegram Desktop `ShowReportFlowBox`,
//! `AddWhoReactedAction`).
use super::*;
use crate::telegram::envelope::{
    AddedReactionsPage, MessageReadDate, MessageViewer, ReactionType, ReportChatOutcome,
};
use crate::text::TextEntityKind;

/// Where the report flow stands. TDLib walks it: an empty `reportChat`
/// answers `OptionRequired` (the reason list), choosing a reason may answer
/// another `OptionRequired` (sub-reasons) or `TextRequired` (details), and
/// the last step answers `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageReportStage {
    /// The first `reportChat` (this request) is in flight.
    Checking(RequestId),
    PickOption {
        title: String,
        options: Vec<ReportOption>,
    },
    /// A follow-up (reason chosen or details sent), this request, is in
    /// flight.
    Sending(RequestId),
    TextRequired {
        option_id: String,
        is_optional: bool,
    },
    Reported,
    Failed(String),
}

impl MessageReportStage {
    /// Whether this stage waits for the answer to `request`.
    fn awaits(&self, request: RequestId) -> bool {
        matches!(self, Self::Checking(id) | Self::Sending(id) if *id == request)
    }
}

/// The in-progress report of one or more messages of a chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReportFlow {
    pub chat_id: ChatId,
    pub message_ids: Vec<MessageId>,
    pub stage: MessageReportStage,
    /// The reasons chosen so far, oldest first.
    pub steps: Vec<ReportStep>,
}

/// One reason chosen in the report flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportStep {
    /// The reason's text, shown as the breadcrumb under the title (tdesktop
    /// shows the chosen reason's text as the next step's title).
    pub chosen: String,
    /// The title and options the step it was chosen in carried, for the
    /// Back button to restore.
    pub title: String,
    pub options: Vec<ReportOption>,
}

/// The poll option a right-click opened the menu on, and what TDLib allows
/// for it. `properties` is `Loading` until `getPollOptionProperties`
/// answers, `Failed` when it is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollOptionMenu {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub option_id: String,
    pub properties: crate::state::fetch::Fetch<crate::telegram::envelope::PollOptionProperties>,
}

/// A stable key of a poll option id, for request purposes that must be
/// `Copy` (FNV-1a, like `reaction_filter_key`).
pub fn poll_option_key(option_id: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in option_id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// One list the audience dialog loads. Each in-flight stage holds the
/// request it waits for: only that request's answer or failure moves it, so
/// an older ask for the same message (the dialog closed and reopened, a tab
/// asked twice) cannot settle the newer one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Audience<T> {
    #[default]
    NotAsked,
    /// The first page is in flight as `request`.
    Loading {
        request: RequestId,
    },
    Ready(T),
    /// Ready, and the next page is in flight as `request`.
    Paging {
        page: T,
        request: RequestId,
    },
    Failed,
}

impl<T> Audience<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) | Self::Paging { page: value, .. } => Some(value),
            _ => None,
        }
    }

    /// The next page of a ready list is on its way.
    pub fn is_paging(&self) -> bool {
        matches!(self, Self::Paging { .. })
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed)
    }

    /// The first page of this list is waiting for `request`.
    fn loading_for(&self, request: RequestId) -> bool {
        matches!(self, Self::Loading { request: asked } if *asked == request)
    }

    /// The list is empty-handed and waits for `request`: the answer of the
    /// first page fills it.
    fn fill(&mut self, request: RequestId, value: T) {
        if self.loading_for(request) {
            *self = Self::Ready(value);
        }
    }

    /// The first page failed, when it was the one waited for.
    fn fail_first(&mut self, request: RequestId) {
        if self.loading_for(request) {
            *self = Self::Failed;
        }
    }
}

/// Who viewed / listened / reacted to one message, for the menu row and
/// its dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageAudience {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub viewers: Audience<Vec<MessageViewer>>,
    pub read_date: Audience<MessageReadDate>,
    pub reactions: Audience<AddedReactionsPage>,
    /// The real author of a post sent on behalf of a chat (`getMessageAuthor`),
    /// as a user id whose name `Session::users` holds.
    pub author: Audience<i64>,
    /// Per-reaction tabs, by `reaction_filter_key` (the "All" tab is
    /// `reactions`).
    pub filtered: HashMap<u64, Audience<AddedReactionsPage>>,
}

impl MessageAudience {
    pub fn new(chat_id: ChatId, message_id: MessageId) -> Self {
        Self {
            chat_id,
            message_id,
            viewers: Audience::NotAsked,
            read_date: Audience::NotAsked,
            reactions: Audience::NotAsked,
            author: Audience::NotAsked,
            filtered: HashMap::new(),
        }
    }
}

/// The sticker set behind "View Sticker Set" / "Add Stickers".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerSetView {
    pub set_id: i64,
    pub stage: StickerSetViewStage,
    /// Edits of an owned set in flight and their outcome.
    pub edits: StickerSetEdits,
    /// The box's Add / Remove / Archive of this set: asked, then answered
    /// by TDLib (the box closes on `Done`, says so on `Failed`).
    pub change: SetChangeStage,
}

/// What the set box asked TDLib to do with the viewed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetChange {
    Install,
    Remove,
    Archive,
}

impl SetChange {
    /// The change a `changeStickerSet` with these flags is.
    pub fn of(installed: bool, archived: bool) -> Self {
        match (installed, archived) {
            (true, _) => Self::Install,
            (false, true) => Self::Archive,
            (false, false) => Self::Remove,
        }
    }
}

/// How far the set box's change got. Every stage has a way out: the answer
/// (`Done`, `Failed`), then the box takes it back to `Idle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SetChangeStage {
    #[default]
    Idle,
    /// The request went out as `request`; only its answer or failure leaves
    /// the stage.
    Working {
        change: SetChange,
        request: RequestId,
    },
    /// TDLib applied it.
    Done(SetChange),
    /// TDLib refused it, or the request was lost.
    Failed(SetChange),
}

impl StickerSetView {
    /// A view that is still loading its set.
    pub fn loading(set_id: i64, request: RequestId) -> Self {
        Self {
            set_id,
            stage: StickerSetViewStage::Loading { request },
            edits: StickerSetEdits::default(),
            change: SetChangeStage::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StickerSetViewStage {
    /// `request` is the `getStickerSet` (or, for the attached sets of a
    /// photo, `getAttachedStickerSets`) the box waits for; only its answer
    /// or failure leaves the stage.
    Loading {
        request: RequestId,
    },
    Ready {
        title: String,
        /// The set's short name, for its `t.me` link (empty if unknown).
        name: String,
        installed: bool,
        /// The set holds custom emoji (`addemoji` link, no archive).
        is_emoji: bool,
        /// The current user created the set (tdesktop `AmCreator`): the
        /// box offers editing.
        is_owned: bool,
        stickers: Vec<StickerItem>,
    },
    Failed,
}

/// What a tapped emoji's pack lookup came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomEmojiAnswer {
    Ready(CustomEmojiPreview),
    Failed,
}

/// The answers of the pack lookups, each under the request that asked, for
/// the window that tapped to take. Never more than a few: a window that
/// closed before it took its answer leaves one behind.
#[derive(Debug, Default)]
pub struct CustomEmojiAnswers(std::collections::VecDeque<(RequestId, CustomEmojiAnswer)>);

impl CustomEmojiAnswers {
    const KEPT: usize = 8;

    fn record(&mut self, request: RequestId, answer: CustomEmojiAnswer) {
        self.0.retain(|(kept, _)| *kept != request);
        if self.0.len() >= Self::KEPT {
            self.0.pop_front();
        }
        self.0.push_back((request, answer));
    }

    /// The answer of `request`, once; `None` while it is still out.
    pub fn take(&mut self, request: RequestId) -> Option<CustomEmojiAnswer> {
        let at = self.0.iter().position(|(kept, _)| *kept == request)?;
        self.0.remove(at).map(|(_, answer)| answer)
    }
}

/// A tapped custom emoji and the pack it comes from (tdesktop's emoji
/// preview, `ShowReactionPreview`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomEmojiPreview {
    pub emoji_id: i64,
    pub set_id: i64,
    pub title: String,
}

impl Session {
    /// The distinct emoji packs of the custom emoji in a message's text
    /// (tdesktop `CollectEmojiPacks`), in order of appearance. Emoji whose
    /// sticker is not loaded yet have no pack to name.
    pub fn message_emoji_pack_ids(&self, content: &MessageContent) -> Vec<i64> {
        let MessageContent::Text(text) = content else {
            return Vec::new();
        };
        let mut sets: Vec<i64> = Vec::new();
        for entity in &text.entities {
            let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                continue;
            };
            let set_id = self
                .stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|s| s.custom_emoji_id == Some(custom_emoji_id))
                .map(|s| s.set_id)
                .unwrap_or(0);
            if set_id != 0 && !sets.contains(&set_id) {
                sets.push(set_id);
            }
        }
        sets
    }

    /// The pack of a tapped custom emoji arrived, for the tap that sent
    /// `request`. A pack with no title has nothing to show.
    pub fn accept_custom_emoji_preview(
        &mut self,
        request: RequestId,
        emoji_id: i64,
        set_id: i64,
        title: String,
    ) {
        let answer = if title.is_empty() {
            CustomEmojiAnswer::Failed
        } else {
            CustomEmojiAnswer::Ready(CustomEmojiPreview {
                emoji_id,
                set_id,
                title,
            })
        };
        self.stickers.custom_emoji_answers.record(request, answer);
    }

    /// The lookup of `request` was refused: its tap shows no card.
    pub(crate) fn fail_custom_emoji_preview(&mut self, request: RequestId) {
        self.stickers
            .custom_emoji_answers
            .record(request, CustomEmojiAnswer::Failed);
    }

    /// The set's contents arrived (`stage` is the `Ready` stage).
    pub(crate) fn accept_sticker_set_view(
        &mut self,
        request: RequestId,
        set_id: i64,
        stage: StickerSetViewStage,
    ) {
        let Some(view) = self.stickers.sticker_set_view.as_mut() else {
            return;
        };
        if view.set_id != set_id {
            return;
        }
        // The load the box waits for, or the refresh after an edit: any
        // other answer (an older load, an older refresh) is not this box's.
        let loading = matches!(view.stage, StickerSetViewStage::Loading { request: waiting } if waiting == request);
        if loading {
            view.edits.load_arrived();
        } else if !view.edits.refresh_arrived(request) {
            return;
        }
        view.stage = stage;
    }

    /// The box's load failed. A refetch after an edit (a rename, an add,
    /// a remove) that took is not a failed load: the box keeps what it
    /// shows.
    pub(crate) fn fail_sticker_set_view(&mut self, request: RequestId) {
        if let Some(view) = self.stickers.sticker_set_view.as_mut()
            && matches!(view.stage, StickerSetViewStage::Loading { request: waiting } if waiting == request)
        {
            view.stage = StickerSetViewStage::Failed;
        } else if let Some(view) = self.stickers.sticker_set_view.as_mut() {
            // A refresh after an edit that failed: the box keeps what it
            // shows.
            view.edits.refresh_failed(request);
        }
    }

    /// Start reporting `message_ids` of `chat_id`; `request` is the first
    /// `reportChat`, the only answer the flow takes while it checks.
    pub fn begin_message_report(
        &mut self,
        chat_id: ChatId,
        message_ids: Vec<MessageId>,
        request: RequestId,
    ) {
        self.messages.message_report = Some(MessageReportFlow {
            chat_id,
            message_ids,
            stage: MessageReportStage::Checking(request),
            steps: Vec::new(),
        });
    }

    pub fn clear_message_report(&mut self) {
        self.messages.message_report = None;
    }

    /// The follow-up `reportChat` (reason chosen or details sent), `request`,
    /// went out.
    pub fn message_report_sending(
        &mut self,
        request: RequestId,
        chosen: Option<(String, String, Vec<ReportOption>)>,
    ) {
        if let Some(flow) = self.messages.message_report.as_mut() {
            if let Some((text, title, options)) = chosen {
                flow.steps.push(ReportStep {
                    chosen: text,
                    title,
                    options,
                });
            }
            flow.stage = MessageReportStage::Sending(request);
        }
    }

    /// Back from a sub-reason list to the previous one.
    pub fn message_report_back(&mut self) -> bool {
        let Some(flow) = self.messages.message_report.as_mut() else {
            return false;
        };
        let Some(ReportStep { title, options, .. }) = flow.steps.pop() else {
            return false;
        };
        flow.stage = MessageReportStage::PickOption { title, options };
        true
    }

    /// Apply a `ReportChatResult` for the flow. Only the answer to the
    /// request the flow is waiting on counts: one for a report that was
    /// cancelled or replaced is dropped.
    pub fn accept_message_report(&mut self, pending: &PendingRequest, outcome: ReportChatOutcome) {
        let Some(flow) = self.messages.message_report.as_mut() else {
            return;
        };
        if !flow.stage.awaits(pending.id) {
            return;
        }
        flow.stage = match outcome {
            ReportChatOutcome::Ok => MessageReportStage::Reported,
            // TDLib's own mapping: no options left means nothing more to ask.
            ReportChatOutcome::OptionRequired { options, .. } if options.is_empty() => {
                MessageReportStage::Reported
            }
            ReportChatOutcome::OptionRequired { title, options } => {
                MessageReportStage::PickOption { title, options }
            }
            ReportChatOutcome::TextRequired {
                option_id,
                is_optional,
            } => MessageReportStage::TextRequired {
                option_id,
                is_optional,
            },
            ReportChatOutcome::MessagesRequired => {
                MessageReportStage::Failed("Telegram needs more messages to report.".into())
            }
        };
    }

    /// A raw TDLib error (or a send failure) of `request` ends the flow
    /// that waits on it.
    pub fn fail_message_report(&mut self, request: RequestId, message: String) {
        if let Some(flow) = self.messages.message_report.as_mut()
            && flow.stage.awaits(request)
        {
            flow.stage = MessageReportStage::Failed(message);
        }
    }

    /// Start a fresh audience record for the message the menu opened on.
    pub fn begin_message_audience(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.messages.message_audience = Some(MessageAudience::new(chat_id, message_id));
    }

    pub(crate) fn audience_for(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<&mut MessageAudience> {
        self.messages
            .message_audience
            .as_mut()
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id)
    }

    pub fn audience_viewers_loading(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        request: RequestId,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.viewers = Audience::Loading { request };
        }
    }

    pub fn audience_read_date_loading(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        request: RequestId,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.read_date = Audience::Loading { request };
        }
    }

    pub fn audience_author_loading(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        request: RequestId,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.author = Audience::Loading { request };
        }
    }

    /// `getMessageAuthor` of `request` answered with this user.
    pub(crate) fn accept_message_author(
        &mut self,
        request: RequestId,
        chat_id: ChatId,
        message_id: MessageId,
        user_id: i64,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.author.fill(request, user_id);
        }
    }

    /// The poll option the menu opens on, with its properties asked as
    /// `request` (`None`: the request could not be sent, so the menu stops
    /// waiting for rows that will not come).
    pub fn begin_poll_option_menu(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_id: &str,
        request: Option<RequestId>,
    ) {
        let mut properties = crate::state::fetch::Fetch::default();
        match request {
            Some(request) => {
                properties.begin();
                properties.sent(request);
            }
            None => properties.fail("the request was not sent"),
        }
        self.messages.poll_option_menu = Some(PollOptionMenu {
            chat_id,
            message_id,
            option_id: option_id.to_string(),
            properties,
        });
    }

    pub fn audience_reactions_loading(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        request: RequestId,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.reactions = Audience::Loading { request };
        }
    }

    pub(crate) fn accept_message_viewers(
        &mut self,
        request: RequestId,
        chat_id: ChatId,
        message_id: MessageId,
        viewers: Vec<MessageViewer>,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.viewers.fill(request, viewers);
        }
    }

    pub(crate) fn accept_message_read_date(
        &mut self,
        request: RequestId,
        chat_id: ChatId,
        message_id: MessageId,
        date: MessageReadDate,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.read_date.fill(request, date);
        }
    }

    /// A page of a "who reacted" tab answered `request`: the first page
    /// fills a tab that waits for it, a further page joins a list that
    /// waits for it. An older ask's page settles neither.
    pub(crate) fn accept_added_reactions(
        &mut self,
        request: RequestId,
        chat_id: ChatId,
        message_id: MessageId,
        filter: u64,
        page: AddedReactionsPage,
    ) {
        let Some(a) = self.audience_for(chat_id, message_id) else {
            return;
        };
        let slot = if filter == 0 {
            Some(&mut a.reactions)
        } else {
            a.filtered.get_mut(&filter)
        };
        let Some(slot) = slot else {
            return;
        };
        match std::mem::take(slot) {
            Audience::Paging {
                page: mut existing,
                request: asked,
            } if asked == request => {
                existing.reactions.extend(page.reactions);
                existing.total_count = page.total_count.max(existing.total_count);
                existing.next_offset = page.next_offset;
                *slot = Audience::Ready(existing);
            }
            Audience::Loading { request: asked } if asked == request => {
                *slot = Audience::Ready(page);
            }
            other => *slot = other,
        }
    }

    /// A tab's first page is on its way as `request`.
    pub fn audience_tab_loading(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        filter: u64,
        request: RequestId,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            if filter == 0 {
                a.reactions = Audience::Loading { request };
            } else {
                a.filtered.insert(filter, Audience::Loading { request });
            }
        }
    }

    /// A tab's next page is on its way as `request`.
    pub fn audience_tab_loading_more(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        filter: u64,
        request: RequestId,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            let slot = if filter == 0 {
                Some(&mut a.reactions)
            } else {
                a.filtered.get_mut(&filter)
            };
            if let Some(slot) = slot {
                *slot = match std::mem::take(slot) {
                    Audience::Ready(page) => Audience::Paging { page, request },
                    other => other,
                };
            }
        }
    }

    /// The audience request `pending` failed (privacy, too old, not a
    /// group...). Only the list that waits for exactly this request fails.
    pub(crate) fn fail_audience(&mut self, pending: &PendingRequest) {
        let request = pending.id;
        match pending.purpose {
            RequestPurpose::Messages(MessagesPurpose::GetMessageViewers {
                chat_id,
                message_id,
            }) => {
                if let Some(a) = self.audience_for(chat_id, message_id) {
                    a.viewers.fail_first(request);
                }
            }
            RequestPurpose::Messages(MessagesPurpose::GetMessageReadDate {
                chat_id,
                message_id,
            }) => {
                if let Some(a) = self.audience_for(chat_id, message_id) {
                    a.read_date.fail_first(request);
                }
            }
            RequestPurpose::Messages(MessagesPurpose::GetMessageAuthor {
                chat_id,
                message_id,
            }) => {
                if let Some(a) = self.audience_for(chat_id, message_id) {
                    a.author.fail_first(request);
                }
            }
            RequestPurpose::Messages(MessagesPurpose::GetMessageAddedReactions {
                chat_id,
                message_id,
                filter,
                ..
            }) => {
                if let Some(a) = self.audience_for(chat_id, message_id) {
                    let slot = if filter == 0 {
                        Some(&mut a.reactions)
                    } else {
                        a.filtered.get_mut(&filter)
                    };
                    let Some(slot) = slot else {
                        return;
                    };
                    // A failed next page keeps what was already listed.
                    match std::mem::take(slot) {
                        Audience::Paging {
                            page,
                            request: asked,
                        } if asked == request => *slot = Audience::Ready(page),
                        Audience::Loading { request: asked } if asked == request => {
                            *slot = Audience::Failed;
                        }
                        other => *slot = other,
                    }
                }
            }
            _ => {}
        }
    }
}

/// Stable key of a reaction for the "who reacted" tabs (0 is "All").
pub fn reaction_filter_key(reaction: &ReactionType) -> u64 {
    let text = match reaction {
        ReactionType::Emoji { emoji } => format!("e:{emoji}"),
        ReactionType::CustomEmoji { custom_emoji_id } => format!("c:{custom_emoji_id}"),
        ReactionType::Paid => return 2,
        ReactionType::Unknown => return 3,
    };
    // FNV-1a: stable across runs, unlike `DefaultHasher`.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash.max(4)
}
