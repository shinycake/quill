//! The composer's AI actions (fix, rewrite, create): each ask is recorded
//! with the draft it was made on, and its answer applies only while that
//! draft is still what the composer shows.
use crate::composer::DraftTopic;
use crate::state::*;

/// What an AI action was asked about: the chat and slot of the composer and
/// the text it showed when the request went out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAsk {
    pub chat_id: ChatId,
    pub topic: DraftTopic,
    pub source: String,
}

/// The body of an answer.
#[derive(Debug, Clone, PartialEq)]
pub enum AiBody {
    /// `fixedText` / `formattedText`: the new draft text.
    Text(String),
    /// A `richMessage` and the note naming the method that answered.
    Blocks(RichMessageContent, &'static str),
}

/// An answer with the ask it answers, waiting for the window that shows
/// the chat.
#[derive(Debug, Clone, PartialEq)]
pub struct AiAnswer {
    pub ask: AiAsk,
    pub body: AiBody,
}

/// The asks in flight and the answers waiting for a composer.
#[derive(Debug, Default)]
pub struct AiComposer {
    asks: HashMap<RequestId, AiAsk>,
    answers: Vec<AiAnswer>,
}

impl AiComposer {
    /// A newer ask on a slot replaces an older one: the older answer would
    /// land on a draft that has moved on.
    fn note(&mut self, request: RequestId, ask: AiAsk) {
        self.asks
            .retain(|_, old| (old.chat_id, old.topic) != (ask.chat_id, ask.topic));
        self.asks.insert(request, ask);
    }

    /// The request answered: its ask joins the waiting answers. An answer
    /// no ask stands behind (replaced, or never noted) is dropped.
    fn answered(&mut self, request: RequestId, body: AiBody) {
        if let Some(ask) = self.asks.remove(&request) {
            self.answers.push(AiAnswer { ask, body });
        }
    }

    /// The request failed or its answer will never come.
    fn forget(&mut self, request: RequestId) {
        self.asks.remove(&request);
    }

    pub fn is_waiting(&self) -> bool {
        !self.answers.is_empty()
    }

    /// Take the answers `wanted` picks, leaving the rest.
    fn take(&mut self, wanted: impl Fn(&AiAnswer) -> bool) -> Vec<AiAnswer> {
        let (taken, kept) = std::mem::take(&mut self.answers)
            .into_iter()
            .partition(|answer| wanted(answer));
        self.answers = kept;
        taken
    }
}

impl Session {
    /// An AI request went out for the composer showing `source`.
    pub fn note_ai_ask(
        &mut self,
        request: RequestId,
        chat_id: ChatId,
        topic: DraftTopic,
        source: String,
    ) {
        self.messages.ai.note(
            request,
            AiAsk {
                chat_id,
                topic,
                source,
            },
        );
    }

    pub fn accept_ai_answer(&mut self, request: RequestId, body: AiBody) {
        self.messages.ai.answered(request, body);
    }

    pub(crate) fn forget_ai_ask(&mut self, request: RequestId) {
        self.messages.ai.forget(request);
    }

    /// The waiting answers one window takes: those for the chat it has
    /// open, and those for a chat no window shows (nothing will ever
    /// apply them). An answer for another window's chat waits for that
    /// window's frame.
    pub fn take_ai_answers_for(&mut self, open_chat: Option<ChatId>) -> Vec<AiAnswer> {
        let ours: Vec<ChatId> = self
            .messages
            .ai
            .answers
            .iter()
            .map(|answer| answer.ask.chat_id)
            .filter(|chat| open_chat == Some(*chat) || !self.chat_is_shown(*chat))
            .collect();
        self.messages
            .ai
            .take(|answer| ours.contains(&answer.ask.chat_id))
    }

    pub fn ai_answers_waiting(&self) -> bool {
        self.messages.ai.is_waiting()
    }
}
