//! Translation state: which chats TDLib says to offer translation for, the
//! translations that were asked for (a whole message through
//! `translateMessageText`, a selection through `translateText`), and the
//! language each chat is currently shown in.
use super::*;
use crate::text::TextEntity;

/// What a translation request is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateTarget {
    /// A whole message (`translateMessageText`).
    Message { chat_id: i64, message_id: i64 },
    /// A piece of text, e.g. a selection (`translateText`).
    Text,
}

/// One translation request in flight, keyed by its job number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslateJob {
    pub target: TranslateTarget,
    pub to_language: String,
}

/// A finished (or failed) translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Translation {
    Pending,
    Done {
        text: String,
        entities: Vec<TextEntity>,
    },
    Failed(String),
}

/// `(chat id, message id, language)`.
pub type MessageTranslationKey = (i64, i64, String);

#[derive(Debug, Default)]
pub struct TranslateState {
    /// Chats with `chat.is_translatable` set.
    pub translatable_chats: HashSet<i64>,
    /// Supergroups with `supergroup.has_automatic_translation` (channels
    /// that show their messages translated for everyone).
    pub auto_translate_supergroups: HashSet<i64>,
    pub jobs: HashMap<u64, TranslateJob>,
    pub next_job: u64,
    pub messages: HashMap<MessageTranslationKey, Translation>,
    /// Selection translations by job number.
    pub texts: HashMap<u64, Translation>,
    /// The language each chat is shown translated into.
    pub chat_to: HashMap<i64, String>,
    /// Bumped on every change a rendered row could depend on.
    pub revision: u64,
}

impl Session {
    pub(crate) fn set_chat_translatable(&mut self, chat_id: i64, translatable: bool) {
        if translatable {
            self.translate.translatable_chats.insert(chat_id);
        } else {
            self.translate.translatable_chats.remove(&chat_id);
        }
    }

    /// Record `supergroup.has_automatic_translation`.
    pub(crate) fn set_supergroup_auto_translate(&mut self, supergroup_id: i64, on: bool) {
        if on {
            self.translate
                .auto_translate_supergroups
                .insert(supergroup_id);
        } else {
            self.translate
                .auto_translate_supergroups
                .remove(&supergroup_id);
        }
    }

    /// `PeerData::autoTranslation`: a channel with automatic translation.
    pub fn chat_auto_translate(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id)
            .is_some_and(|id| self.translate.auto_translate_supergroups.contains(&id))
    }

    /// `chat.is_translatable`: TDLib suggests translating this chat.
    pub fn chat_is_translatable(&self, chat_id: ChatId) -> bool {
        self.translate.translatable_chats.contains(&chat_id.0)
    }

    /// Whether the account has Telegram Premium (`is_premium` option).
    pub fn is_premium(&self) -> bool {
        self.premium_option.unwrap_or(false)
    }

    /// Register a translation job and the TDLib request that serves it.
    /// Returns the job number and the request id to send with.
    pub fn begin_translation(
        &mut self,
        target: TranslateTarget,
        to_language: &str,
        chat_id: Option<ChatId>,
    ) -> (u64, RequestId) {
        self.translate.next_job += 1;
        let job = self.translate.next_job;
        match &target {
            TranslateTarget::Message {
                chat_id,
                message_id,
            } => {
                self.translate.messages.insert(
                    (*chat_id, *message_id, to_language.to_string()),
                    Translation::Pending,
                );
            }
            TranslateTarget::Text => {
                self.translate.texts.insert(job, Translation::Pending);
            }
        }
        self.translate.jobs.insert(
            job,
            TranslateJob {
                target,
                to_language: to_language.to_string(),
            },
        );
        self.translate.revision += 1;
        let extra = self.request(
            RequestPurpose::Messages(MessagesPurpose::TranslateJob { job }),
            chat_id,
        );
        (job, extra)
    }

    /// Roll back a job whose request could not be sent.
    pub fn abandon_translation(&mut self, job: u64, extra: RequestId) {
        self.requests.take(extra);
        self.finish_translation(job, Translation::Failed("send".into()));
    }

    /// Record the answer (or failure) of a job.
    pub(crate) fn finish_translation(&mut self, job: u64, result: Translation) {
        let Some(finished) = self.translate.jobs.remove(&job) else {
            return;
        };
        match finished.target {
            TranslateTarget::Message {
                chat_id,
                message_id,
            } => {
                self.translate
                    .messages
                    .insert((chat_id, message_id, finished.to_language), result);
            }
            TranslateTarget::Text => {
                self.translate.texts.insert(job, result);
            }
        }
        self.translate.revision += 1;
    }

    /// The translation of a message into `to_language`, if asked for.
    pub fn message_translation(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        to_language: &str,
    ) -> Option<&Translation> {
        self.translate
            .messages
            .get(&(chat_id.0, message_id.0, to_language.to_string()))
    }

    /// The translation of a selection job.
    pub fn text_translation(&self, job: u64) -> Option<&Translation> {
        self.translate.texts.get(&job)
    }

    /// Forget a finished selection translation.
    pub fn drop_text_translation(&mut self, job: u64) {
        self.translate.texts.remove(&job);
        self.translate.jobs.remove(&job);
    }

    /// The language the chat is shown translated into, if it is.
    pub fn chat_translated_to(&self, chat_id: ChatId) -> Option<&str> {
        self.translate.chat_to.get(&chat_id.0).map(String::as_str)
    }

    /// Show the chat translated into `to`, or back in the original.
    pub fn set_chat_translated_to(&mut self, chat_id: ChatId, to: Option<&str>) {
        let changed = match to {
            Some(to) => {
                self.translate
                    .chat_to
                    .insert(chat_id.0, to.to_string())
                    .as_deref()
                    != Some(to)
            }
            None => self.translate.chat_to.remove(&chat_id.0).is_some(),
        };
        if changed {
            self.translate.revision += 1;
        }
    }

    /// Whether `message_id` still needs a translation into `to`.
    pub fn needs_translation(&self, chat_id: ChatId, message_id: MessageId, to: &str) -> bool {
        self.message_translation(chat_id, message_id, to).is_none()
    }
}
