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

/// How many times the translated chat's pump asks for a message again
/// after failures (a send refused while offline, a restart, a TDLib
/// refusal); then the message keeps its original text until the person asks.
pub const TRANSLATE_PUMP_ATTEMPTS: u8 = 3;

/// A translation: asked for (by its request), finished, or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Translation {
    /// `request` is out; its answer finds this entry. `attempts` are the
    /// failures before it.
    Pending { request: RequestId, attempts: u8 },
    Done {
        text: String,
        entities: Vec<TextEntity>,
    },
    /// The request failed; `attempts` counts failures so far.
    Failed { reason: String, attempts: u8 },
}

impl Translation {
    /// A failed translation with no count yet; `finish_translation` adds
    /// the failures that came before it.
    pub fn failed(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            attempts: 0,
        }
    }
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
    pub messages: HashMap<MessageTranslationKey, Translation>,
    /// Selection translations by the request that asked.
    pub texts: HashMap<RequestId, Translation>,
    /// The language each chat is shown translated into.
    pub chat_to: HashMap<i64, String>,
    /// Bumped on every change a rendered row could depend on.
    pub revision: u64,
}

impl TranslateState {
    /// The requests of the translations still asking.
    pub fn pending_requests(&self) -> impl Iterator<Item = RequestId> + '_ {
        self.messages
            .values()
            .chain(self.texts.values())
            .filter_map(|translation| match translation {
                Translation::Pending { request, .. } => Some(*request),
                _ => None,
            })
    }

    /// How many translations are in flight.
    pub fn in_flight(&self) -> usize {
        self.pending_requests().count()
    }
}

impl Session {
    pub(crate) fn set_chat_translatable(&mut self, chat_id: i64, translatable: bool) {
        if translatable {
            self.messages.translate.translatable_chats.insert(chat_id);
        } else {
            self.messages.translate.translatable_chats.remove(&chat_id);
        }
    }

    /// Record `supergroup.has_automatic_translation`.
    pub(crate) fn set_supergroup_auto_translate(&mut self, supergroup_id: i64, on: bool) {
        if on {
            self.messages
                .translate
                .auto_translate_supergroups
                .insert(supergroup_id);
        } else {
            self.messages
                .translate
                .auto_translate_supergroups
                .remove(&supergroup_id);
        }
    }

    /// `PeerData::autoTranslation`: a channel with automatic translation.
    pub fn chat_auto_translate(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.messages
                .translate
                .auto_translate_supergroups
                .contains(&id)
        })
    }

    /// `chat.is_translatable`: TDLib suggests translating this chat.
    pub fn chat_is_translatable(&self, chat_id: ChatId) -> bool {
        self.messages
            .translate
            .translatable_chats
            .contains(&chat_id.0)
    }

    /// Whether the account has Telegram Premium (`is_premium` option).
    pub fn is_premium(&self) -> bool {
        self.payments.premium_option.unwrap_or(false)
    }

    /// Register a translation and the TDLib request that serves it. Returns
    /// the request id to send with; it also names a selection's translation.
    pub fn begin_translation(
        &mut self,
        target: TranslateTarget,
        to_language: &str,
        chat_id: Option<ChatId>,
    ) -> RequestId {
        let request = self.request(
            RequestPurpose::Messages(MessagesPurpose::TranslateJob),
            chat_id,
        );
        match &target {
            TranslateTarget::Message {
                chat_id,
                message_id,
            } => {
                let key = (*chat_id, *message_id, to_language.to_string());
                let attempts = match self.messages.translate.messages.get(&key) {
                    Some(Translation::Failed { attempts, .. }) => *attempts,
                    _ => 0,
                };
                self.messages
                    .translate
                    .messages
                    .insert(key, Translation::Pending { request, attempts });
            }
            TranslateTarget::Text => {
                self.messages.translate.texts.insert(
                    request,
                    Translation::Pending {
                        request,
                        attempts: 0,
                    },
                );
            }
        }
        self.messages.translate.revision += 1;
        request
    }

    /// Roll back a translation whose request could not be sent.
    pub fn abandon_translation(&mut self, request: RequestId) {
        self.requests.take(request);
        self.finish_translation(request, Translation::failed("send"));
    }

    /// "Load full message" of this message is out.
    pub fn full_rich_loading(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.messages
            .full_rich_loads
            .get(&crate::ids::ChatMessage::new(chat_id, message_id))
            .is_some_and(|load| load.is_loading())
    }

    /// Translations whose request is gone without an answer (a dropped
    /// continuation, a restarted client) fail, so they stop holding a place
    /// in the pump's in-flight cap and their messages are not "pending"
    /// for good.
    pub fn fail_orphaned_translations(&mut self) {
        let orphans: Vec<RequestId> = self
            .messages
            .translate
            .pending_requests()
            .filter(|request| self.requests.get(*request).is_none())
            .collect();
        for request in orphans {
            self.finish_translation(request, Translation::failed("lost"));
        }
    }

    /// Record the answer (or failure) of `request`; an entry another
    /// request has replaced ignores it.
    pub fn finish_translation(&mut self, request: RequestId, result: Translation) {
        let translate = &mut self.messages.translate;
        let is_mine = |slot: &Translation| matches!(slot, Translation::Pending { request: asked, .. } if *asked == request);
        let slot = match translate.texts.get_mut(&request) {
            Some(slot) if is_mine(slot) => Some(slot),
            _ => translate.messages.values_mut().find(|slot| is_mine(slot)),
        };
        let Some(slot) = slot else {
            return;
        };
        let before = match slot {
            Translation::Pending { attempts, .. } => *attempts,
            _ => 0,
        };
        *slot = match result {
            Translation::Failed { reason, .. } => Translation::Failed {
                reason,
                attempts: before.saturating_add(1),
            },
            done => done,
        };
        translate.revision += 1;
    }

    /// The translation of a message into `to_language`, if asked for.
    pub fn message_translation(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        to_language: &str,
    ) -> Option<&Translation> {
        self.messages
            .translate
            .messages
            .get(&(chat_id.0, message_id.0, to_language.to_string()))
    }

    /// The chat is opened again: translations of its messages that failed
    /// are asked for again (an answer that failed earlier, offline or on
    /// a refused send, is not final). True when any was dropped.
    pub fn retry_failed_translations(&mut self, chat_id: ChatId) -> bool {
        let translate = &mut self.messages.translate;
        let before = translate.messages.len();
        translate.messages.retain(|(chat, _, _), translation| {
            *chat != chat_id.0 || !matches!(translation, Translation::Failed { .. })
        });
        let dropped = translate.messages.len() != before;
        if dropped {
            translate.revision += 1;
        }
        dropped
    }

    /// The translation a selection request asked for.
    pub fn text_translation(&self, request: RequestId) -> Option<&Translation> {
        self.messages.translate.texts.get(&request)
    }

    /// Forget a selection translation; its answer, if still to come,
    /// finds nothing.
    pub fn drop_text_translation(&mut self, request: RequestId) {
        self.messages.translate.texts.remove(&request);
    }

    /// The language the chat is shown translated into, if it is.
    pub fn chat_translated_to(&self, chat_id: ChatId) -> Option<&str> {
        self.messages
            .translate
            .chat_to
            .get(&chat_id.0)
            .map(String::as_str)
    }

    /// Show the chat translated into `to`, or back in the original.
    pub fn set_chat_translated_to(&mut self, chat_id: ChatId, to: Option<&str>) {
        let changed = match to {
            Some(to) => {
                self.messages
                    .translate
                    .chat_to
                    .insert(chat_id.0, to.to_string())
                    .as_deref()
                    != Some(to)
            }
            None => self.messages.translate.chat_to.remove(&chat_id.0).is_some(),
        };
        if changed {
            self.messages.translate.revision += 1;
        }
    }

    /// The message's text changed: its translations into every language
    /// go, and so do jobs still asking for one, so an answer for the old
    /// text lands nowhere (tdesktop `HistoryItem::setTextValue` removes
    /// `HistoryMessageTranslation`). A translated chat asks again.
    pub(crate) fn forget_message_translations(&mut self, chat_id: ChatId, message_id: MessageId) {
        let translate = &mut self.messages.translate;
        let before = translate.messages.len();
        translate
            .messages
            .retain(|(chat, message, _), _| (*chat, *message) != (chat_id.0, message_id.0));
        if translate.messages.len() != before {
            translate.revision += 1;
        }
    }

    /// A chat's history shrank to a tail: the finished translations (and
    /// failures) of messages it no longer holds go with them. A request
    /// still out keeps its entry, which its answer finds.
    pub(crate) fn prune_message_translations(&mut self, chat_id: ChatId) {
        let held = |session: &Session, message: i64| {
            session
                .histories
                .get(&chat_id.0)
                .is_some_and(|history| history.messages.contains_key(&message))
                || session
                    .threads
                    .topic_histories
                    .iter()
                    .any(|((chat, _), topic)| {
                        *chat == chat_id.0 && topic.messages.contains_key(&message)
                    })
                || session.views_shown().any(|view| {
                    view.thread.is_some_and(|thread| {
                        thread.chat_id == chat_id && thread.history.messages.contains_key(&message)
                    })
                })
        };
        let gone: Vec<MessageTranslationKey> = self
            .messages
            .translate
            .messages
            .iter()
            .filter(|((chat, message, _), translation)| {
                *chat == chat_id.0
                    && !matches!(translation, Translation::Pending { .. })
                    && !held(self, *message)
            })
            .map(|(key, _)| key.clone())
            .collect();
        if gone.is_empty() {
            return;
        }
        for key in gone {
            self.messages.translate.messages.remove(&key);
        }
        self.messages.translate.revision += 1;
    }

    /// Whether `message_id` has no finished translation into `to`: what
    /// the pump keeps in its scan. A `Pending` ask is kept too, so a
    /// failure that ran out of attempts is tested again once they are
    /// given back ([`Self::translations_back_online`]); `needs_translation`
    /// keeps the pending ask from being sent twice.
    pub fn translation_unsettled(&self, chat_id: ChatId, message_id: MessageId, to: &str) -> bool {
        !matches!(
            self.message_translation(chat_id, message_id, to),
            Some(Translation::Done { .. })
        )
    }

    /// The connection came back: every failed translation gets its
    /// attempts again, so the pump of a chat that stayed open asks once
    /// more (a failure that ran out of attempts offline is not final).
    pub fn translations_back_online(&mut self) {
        let translate = &mut self.messages.translate;
        let mut changed = false;
        for translation in translate.messages.values_mut() {
            if let Translation::Failed { attempts, .. } = translation
                && *attempts != 0
            {
                *attempts = 0;
                changed = true;
            }
        }
        if changed {
            translate.revision += 1;
        }
    }

    /// Whether the translated chat's pump should ask for `message_id` into
    /// `to`: nothing was asked yet, or the asks failed fewer than
    /// `TRANSLATE_PUMP_ATTEMPTS` times (`ConnectDriver::translate_message`
    /// asks a failed one again).
    pub fn needs_translation(&self, chat_id: ChatId, message_id: MessageId, to: &str) -> bool {
        match self.message_translation(chat_id, message_id, to) {
            None => true,
            Some(Translation::Failed { attempts, .. }) => *attempts < TRANSLATE_PUMP_ATTEMPTS,
            Some(Translation::Pending { .. } | Translation::Done { .. }) => false,
        }
    }
}
