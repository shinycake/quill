//! State behind the message context menu's "Report" flow and its
//! "N Seen" / "N Reacted" lists (Telegram Desktop `ShowReportFlowBox`,
//! `AddWhoReactedAction`).
use super::*;
use crate::telegram::envelope::{
    AddedReactionsPage, MessageReadDate, MessageViewer, ReportChatOutcome,
};

/// Where the report flow stands. TDLib walks it: an empty `reportChat`
/// answers `OptionRequired` (the reason list), choosing a reason may answer
/// another `OptionRequired` (sub-reasons) or `TextRequired` (details), and
/// the last step answers `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageReportStage {
    /// The first `reportChat` is in flight.
    Checking,
    PickOption {
        title: String,
        options: Vec<ReportOption>,
    },
    /// A follow-up (reason chosen or details sent) is in flight.
    Sending,
    TextRequired {
        option_id: String,
        is_optional: bool,
    },
    Reported,
    Failed(String),
}

/// The in-progress report of one or more messages of a chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReportFlow {
    pub chat_id: ChatId,
    pub message_ids: Vec<MessageId>,
    pub stage: MessageReportStage,
    /// Titles of the reasons chosen so far, shown as the breadcrumb under
    /// the title (tdesktop shows the chosen reason's text as the next
    /// step's title).
    pub trail: Vec<String>,
    /// Title the previous step's `OptionRequired` carried, for the Back
    /// button to restore.
    pub previous: Vec<(String, Vec<ReportOption>)>,
}

/// One list the audience dialog loads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Audience<T> {
    #[default]
    NotAsked,
    Loading,
    Ready(T),
    Failed,
}

impl<T> Audience<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
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
}

impl MessageAudience {
    pub fn new(chat_id: ChatId, message_id: MessageId) -> Self {
        Self {
            chat_id,
            message_id,
            viewers: Audience::NotAsked,
            read_date: Audience::NotAsked,
            reactions: Audience::NotAsked,
        }
    }
}

impl Session {
    /// Start reporting `message_ids` of `chat_id`.
    pub fn begin_message_report(&mut self, chat_id: ChatId, message_ids: Vec<MessageId>) {
        self.message_report = Some(MessageReportFlow {
            chat_id,
            message_ids,
            stage: MessageReportStage::Checking,
            trail: Vec::new(),
            previous: Vec::new(),
        });
    }

    pub fn clear_message_report(&mut self) {
        self.message_report = None;
    }

    /// The follow-up `reportChat` (reason chosen or details sent) went out.
    pub fn message_report_sending(&mut self, chosen: Option<(String, String, Vec<ReportOption>)>) {
        if let Some(flow) = self.message_report.as_mut() {
            if let Some((text, title, options)) = chosen {
                flow.trail.push(text);
                flow.previous.push((title, options));
            }
            flow.stage = MessageReportStage::Sending;
        }
    }

    /// Back from a sub-reason list to the previous one.
    pub fn message_report_back(&mut self) -> bool {
        let Some(flow) = self.message_report.as_mut() else {
            return false;
        };
        let Some((title, options)) = flow.previous.pop() else {
            return false;
        };
        flow.trail.pop();
        flow.stage = MessageReportStage::PickOption { title, options };
        true
    }

    /// Apply a `ReportChatResult` for the flow. Stale answers (another
    /// chat) are dropped.
    pub fn accept_message_report(&mut self, pending: &PendingRequest, outcome: ReportChatOutcome) {
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let Some(flow) = self.message_report.as_mut() else {
            return;
        };
        if flow.chat_id != chat_id {
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

    /// A raw TDLib error (or a send failure) ends the flow.
    pub fn fail_message_report(&mut self, chat_id: ChatId, message: String) {
        if let Some(flow) = self.message_report.as_mut()
            && flow.chat_id == chat_id
        {
            flow.stage = MessageReportStage::Failed(message);
        }
    }

    /// Start a fresh audience record for the message the menu opened on.
    pub fn begin_message_audience(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.message_audience = Some(MessageAudience::new(chat_id, message_id));
    }

    fn audience_for(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<&mut MessageAudience> {
        self.message_audience
            .as_mut()
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id)
    }

    pub fn audience_viewers_loading(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.viewers = Audience::Loading;
        }
    }

    pub fn audience_read_date_loading(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.read_date = Audience::Loading;
        }
    }

    pub fn audience_reactions_loading(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.reactions = Audience::Loading;
        }
    }

    pub(crate) fn accept_message_viewers(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        viewers: Vec<MessageViewer>,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.viewers = Audience::Ready(viewers);
        }
    }

    pub(crate) fn accept_message_read_date(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        date: MessageReadDate,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.read_date = Audience::Ready(date);
        }
    }

    pub(crate) fn accept_added_reactions(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        page: AddedReactionsPage,
    ) {
        if let Some(a) = self.audience_for(chat_id, message_id) {
            a.reactions = Audience::Ready(page);
        }
    }

    /// The audience request failed (privacy, too old, not a group...).
    pub(crate) fn fail_audience(&mut self, purpose: RequestPurpose) {
        match purpose {
            RequestPurpose::GetMessageViewers {
                chat_id,
                message_id,
            } => {
                if let Some(a) = self.audience_for(chat_id, message_id) {
                    a.viewers = Audience::Failed;
                }
            }
            RequestPurpose::GetMessageReadDate {
                chat_id,
                message_id,
            } => {
                if let Some(a) = self.audience_for(chat_id, message_id) {
                    a.read_date = Audience::Failed;
                }
            }
            RequestPurpose::GetMessageAddedReactions {
                chat_id,
                message_id,
            } => {
                if let Some(a) = self.audience_for(chat_id, message_id) {
                    a.reactions = Audience::Failed;
                }
            }
            _ => {}
        }
    }
}
