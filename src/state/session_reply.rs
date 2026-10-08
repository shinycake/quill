//! Bubble headers: the reply strip, the forward line and "via @bot"
//! (tdesktop `HistoryMessageReply`, `HistoryMessageForwarded`,
//! `HistoryMessageVia`; `history/history_item_components.cpp`).
use super::*;
use crate::ids::UserId;

/// The replied-to message of a loaded row when it is not in the loaded
/// history window, fetched with `getRepliedMessage` (tdesktop
/// `HistoryItem::requestReply`). Keyed by the *replying* message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyTarget {
    /// `getRepliedMessage` sent, answer pending.
    Loading,
    /// The answer: the replied message (own chat or another one).
    Loaded(Box<HistoryMessage>),
    /// TDLib said it does not exist (deleted, or no access).
    Missing,
}

/// How far a reply strip got in resolving its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyState {
    Ready,
    Loading,
    Deleted,
}

/// Everything the UI needs to draw a reply strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyHeader {
    pub target_chat: ChatId,
    pub target_id: MessageId,
    pub state: ReplyState,
    /// The replied message's sender (user, channel or hidden name).
    pub name: Option<String>,
    /// Telegram name color id of that sender, when known.
    pub accent: Option<i32>,
    /// Quote, else a one-line preview of the original, else the state
    /// text ("Deleted message", "Loading…").
    pub text: String,
    pub is_quote: bool,
    /// Title of the other chat for replies from elsewhere ("Name › Chat").
    pub external_chat: Option<String>,
    /// Candidate files for the media thumbnail (best first).
    pub thumb: Vec<FileId>,
    /// Whether a click can take the user to the original.
    pub clickable: bool,
}

impl ReplyHeader {
    /// The name line of the strip: "Name", or "Name › Chat" for a reply
    /// from another chat.
    pub fn name_line(&self) -> Option<String> {
        match (&self.name, &self.external_chat) {
            (Some(name), Some(chat)) => Some(format!("{name} › {chat}")),
            (Some(name), None) => Some(name.clone()),
            (None, Some(chat)) => Some(chat.clone()),
            (None, None) => None,
        }
    }
}

/// Where a forward header leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardLink {
    /// Origin has no reachable profile (hidden account, unknown chat).
    None,
    /// The account was hidden by the user: tooltip only.
    Hidden,
    /// Imported from another app: a toast, no destination.
    Imported,
    User(UserId),
    /// A chat or channel; `message_id` jumps to the original post.
    Chat {
        chat_id: ChatId,
        message_id: Option<MessageId>,
    },
}

/// The "Forwarded from <name>" line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardHeader {
    pub name: String,
    /// Channel post author, shown as "Channel (Author)".
    pub signature: Option<String>,
    pub accent: Option<i32>,
    pub link: ForwardLink,
    /// Origin send date (unix seconds, 0 when unknown).
    pub original_date: i32,
    pub imported: bool,
}

impl ForwardHeader {
    /// Name with the channel author signature (`lng_forwarded_signed`).
    pub fn display_name(&self) -> String {
        match &self.signature {
            Some(signature) => format!("{} ({signature})", self.name),
            None => self.name.clone(),
        }
    }

    /// Hover text of the name (`lng_forwarded_hidden`,
    /// `lng_forwarded_imported`).
    pub fn tooltip(&self) -> Option<&'static str> {
        match self.link {
            ForwardLink::Hidden => Some("The account was hidden by the user."),
            ForwardLink::Imported => {
                Some("This message was imported from another app. It may not be real.")
            }
            _ => None,
        }
    }
}

/// Files worth showing as a small thumbnail of `content` (best first).
pub fn thumb_candidates(content: &MessageContent) -> Vec<FileId> {
    let mut ids = Vec::new();
    match content {
        MessageContent::Photo(photo) if !photo.is_secret && !photo.has_spoiler => {
            ids.extend(photo.thumb_size().map(|size| size.file_id));
            ids.extend(photo.largest_size().map(|size| size.file_id));
        }
        MessageContent::Video(video) if !video.is_secret && !video.has_spoiler => {
            ids.extend(video.thumb_file_id());
        }
        MessageContent::Animation(animation)
            if !animation.is_secret && !animation.has_spoiler =>
        {
            ids.extend(animation.thumb_file_id());
        }
        MessageContent::VideoNote(note) if !note.is_secret => {
            ids.extend(note.thumb_file_id());
        }
        MessageContent::Sticker(sticker) => ids.extend(sticker.display_file_id()),
        _ => {}
    }
    ids.retain(|id| id.0 != 0);
    ids.dedup();
    ids
}

/// Replies the loader asks for per ingest, so one huge history does not
/// flood TDLib.
const REPLY_FETCH_BATCH: usize = 24;

impl Session {
    /// Display name and name-color id of a message sender.
    pub fn sender_name_and_accent(
        &self,
        sender: Option<MessageSender>,
    ) -> (Option<String>, Option<i32>) {
        match sender {
            Some(MessageSender::User { user_id }) => match self.user(user_id) {
                Some(user) => (Some(user.display_name()), Some(user.accent_color_id)),
                None => (None, None),
            },
            Some(MessageSender::Chat { chat_id }) => (
                self.chats.get(&chat_id).map(|chat| chat.title.clone()),
                Some(chat_id.rem_euclid(7) as i32),
            ),
            None => (None, None),
        }
    }

    fn origin_name_and_accent(&self, origin: &MessageOrigin) -> (Option<String>, Option<i32>) {
        match origin {
            MessageOrigin::User { user_id } => {
                self.sender_name_and_accent(Some(MessageSender::User { user_id: user_id.0 }))
            }
            MessageOrigin::HiddenUser { sender_name } => {
                ((!sender_name.trim().is_empty()).then(|| sender_name.clone()), None)
            }
            MessageOrigin::Chat { chat_id, .. } | MessageOrigin::Channel { chat_id, .. } => {
                self.sender_name_and_accent(Some(MessageSender::Chat { chat_id: chat_id.0 }))
            }
        }
    }

    /// The reply strip of `message`, or `None` when it replies to nothing.
    pub fn reply_header(&self, message: &HistoryMessage) -> Option<ReplyHeader> {
        let reply = message.reply_to.as_ref()?;
        let target_chat = if reply.chat_id.0 == 0 {
            message.chat_id
        } else {
            reply.chat_id
        };
        let external = target_chat != message.chat_id;
        let fetched = self.reply_targets.get(&(message.chat_id.0, message.id.0));
        let original: Option<&HistoryMessage> = self
            .histories
            .get(&target_chat.0)
            .and_then(|history| history.messages.get(&reply.message_id.0))
            .or(match fetched {
                Some(ReplyTarget::Loaded(found)) => Some(found.as_ref()),
                _ => None,
            });
        let tombstoned = self
            .histories
            .get(&target_chat.0)
            .is_some_and(|history| history.is_tombstone(reply.message_id));
        let quote = reply
            .quote_text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let original_preview = original.map(effective_preview);
        let preview = original_preview
            .clone()
            .or_else(|| reply.content_preview.clone());
        let (name, accent) = match (original, &reply.origin) {
            (Some(found), _) => self.sender_name_and_accent(found.sender),
            (None, Some(origin)) => self.origin_name_and_accent(origin),
            (None, None) => (None, None),
        };
        let name = name.or_else(|| {
            original
                .filter(|found| found.is_outgoing)
                .map(|_| "You".to_string())
        });
        let state = if original.is_some() || preview.is_some() || quote.is_some() {
            ReplyState::Ready
        } else if tombstoned || matches!(fetched, Some(ReplyTarget::Missing)) {
            ReplyState::Deleted
        } else {
            ReplyState::Loading
        };
        let text = match (state, quote) {
            (_, Some(quote)) => quote.to_string(),
            (ReplyState::Ready, None) => preview.unwrap_or_default(),
            (ReplyState::Loading, None) => "Loading…".to_string(),
            (ReplyState::Deleted, None) => "Deleted message".to_string(),
        };
        let thumb = if quote.is_some() {
            Vec::new()
        } else if let Some(found) = original {
            thumb_candidates(effective_content(&found.content, found.ephemeral.as_ref()))
        } else {
            reply
                .content
                .as_deref()
                .map(thumb_candidates)
                .unwrap_or_default()
        };
        let external_chat = external
            .then(|| self.chats.get(&target_chat.0).map(|chat| chat.title.clone()))
            .flatten();
        let clickable = state == ReplyState::Ready
            && (!external || self.chats.contains_key(&target_chat.0));
        Some(ReplyHeader {
            target_chat,
            target_id: reply.message_id,
            state,
            name,
            accent,
            text,
            is_quote: quote.is_some(),
            external_chat,
            thumb,
            clickable,
        })
    }

    /// The "Forwarded from" line of `message` (a real forward, or an
    /// imported message which tdesktop draws as a forward from the
    /// imported sender).
    pub fn forward_header(&self, message: &HistoryMessage) -> Option<ForwardHeader> {
        if let Some(info) = &message.forward_info {
            let (name, signature, accent, link) = match &info.origin {
                MessageOrigin::HiddenUser { sender_name } => (
                    Some(sender_name.trim().to_string()).filter(|name| !name.is_empty()),
                    None,
                    None,
                    ForwardLink::Hidden,
                ),
                MessageOrigin::User { user_id } => {
                    let (name, accent) = self.origin_name_and_accent(&info.origin);
                    (name, None, accent, ForwardLink::User(*user_id))
                }
                MessageOrigin::Chat {
                    chat_id,
                    author_signature,
                } => {
                    let (name, accent) = self.origin_name_and_accent(&info.origin);
                    (
                        name,
                        Some(author_signature.clone()).filter(|text| !text.is_empty()),
                        accent,
                        ForwardLink::Chat {
                            chat_id: *chat_id,
                            message_id: None,
                        },
                    )
                }
                MessageOrigin::Channel {
                    chat_id,
                    message_id,
                    author_signature,
                } => {
                    let (name, accent) = self.origin_name_and_accent(&info.origin);
                    (
                        name,
                        Some(author_signature.clone()).filter(|text| !text.is_empty()),
                        accent,
                        ForwardLink::Chat {
                            chat_id: *chat_id,
                            message_id: (message_id.0 != 0).then_some(*message_id),
                        },
                    )
                }
            };
            // An origin the chat list does not know falls back to the
            // channel signature; with nothing left the line reads
            // "Forwarded message" and leads nowhere.
            let (name, signature, link) = match name {
                Some(name) => (name, signature, link),
                None => (signature.unwrap_or_default(), None, ForwardLink::None),
            };
            let link = if name.is_empty() { ForwardLink::None } else { link };
            return Some(ForwardHeader {
                name,
                signature,
                accent,
                link,
                original_date: info.date,
                imported: false,
            });
        }
        let import = message.extras.import_info.as_ref()?;
        Some(ForwardHeader {
            name: import.sender_name.clone(),
            signature: None,
            accent: None,
            link: ForwardLink::Imported,
            original_date: import.date,
            imported: true,
        })
    }

    /// `via @bot` for a message sent through an inline bot, when the bot's
    /// username is known.
    pub fn via_bot_label(&self, message: &HistoryMessage) -> Option<String> {
        let bot = message.extras.via_bot_user_id;
        if bot == 0 {
            return None;
        }
        self.user(bot)
            .map(|user| user.username.clone())
            .filter(|username| !username.is_empty())
            .map(|username| format!("@{username}"))
    }

    /// Rows of the open chat (and its loaded topics) whose replied-to
    /// message is neither loaded, known deleted nor already requested.
    pub fn reply_fetch_candidates(&self) -> Vec<(ChatId, MessageId)> {
        let Some(open) = self.open_chat else {
            return Vec::new();
        };
        let mut wanted = Vec::new();
        let rows = self
            .histories
            .get(&open.0)
            .into_iter()
            .flat_map(|history| history.messages.values())
            .chain(
                self.topic_histories
                    .iter()
                    .filter(|((chat, _), _)| *chat == open.0)
                    .flat_map(|(_, topic)| topic.messages.values()),
            );
        for message in rows {
            if message.id.0 <= 0 || message.pending {
                continue;
            }
            let Some(reply) = &message.reply_to else {
                continue;
            };
            if self
                .reply_targets
                .contains_key(&(message.chat_id.0, message.id.0))
            {
                continue;
            }
            let target_chat = if reply.chat_id.0 == 0 {
                message.chat_id
            } else {
                reply.chat_id
            };
            let known = self.histories.get(&target_chat.0).is_some_and(|history| {
                history.contains(reply.message_id) || history.is_tombstone(reply.message_id)
            });
            if known || (target_chat != message.chat_id && reply.content_preview.is_some()) {
                continue;
            }
            // A quote carries its own text; the original is only needed for
            // the sender name, which is worth one request too.
            wanted.push((message.chat_id, message.id));
            if wanted.len() >= REPLY_FETCH_BATCH {
                break;
            }
        }
        wanted
    }

    /// `getRepliedMessage` answer: keep the message beside the history
    /// (it never enters the loaded window).
    pub(crate) fn accept_replied_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        message: ParsedMessage,
    ) {
        self.remember_files(&message.files);
        self.reply_targets.insert(
            (chat_id.0, message_id.0),
            ReplyTarget::Loaded(Box::new(history_message(message, false))),
        );
    }

    /// `getRepliedMessage` failed: the original is gone or unreachable.
    pub(crate) fn reject_replied_message(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.reply_targets
            .insert((chat_id.0, message_id.0), ReplyTarget::Missing);
    }

    /// Thumbnails of fetched reply targets and other-chat replies that the
    /// bubble strips draw.
    pub(crate) fn reply_thumb_file_ids(&self) -> Vec<FileId> {
        let mut ids = Vec::new();
        for target in self.reply_targets.values() {
            if let ReplyTarget::Loaded(found) = target {
                ids.extend(thumb_candidates(effective_content(
                    &found.content,
                    found.ephemeral.as_ref(),
                )));
            }
        }
        if let Some(history) = self
            .open_chat
            .and_then(|chat| self.histories.get(&chat.0))
        {
            for message in history.messages.values() {
                if let Some(content) = message
                    .reply_to
                    .as_ref()
                    .and_then(|reply| reply.content.as_deref())
                {
                    ids.extend(thumb_candidates(content));
                }
            }
        }
        ids
    }
}

/// Hover text of a bubble's time (tdesktop `Element::dateTooltipText`):
/// the send date, then `Edited:` and, for forwards, `Original:` dates.
/// Imported messages open with the imported-message warning.
pub fn footer_tooltip(
    message: &HistoryMessage,
    format_date: impl Fn(i64) -> String,
) -> Option<String> {
    if message.date <= 0 {
        return None;
    }
    let mut text = format_date(i64::from(message.date));
    if message.extras.edit_date > 0 {
        text.push_str(&format!(
            "\nEdited: {}",
            format_date(i64::from(message.extras.edit_date))
        ));
    }
    let original = message
        .forward_info
        .as_ref()
        .map(|info| info.date)
        .or(message.extras.import_info.as_ref().map(|info| info.date))
        .filter(|date| *date > 0);
    if let Some(original) = original {
        text.push_str(&format!("\nOriginal: {}", format_date(i64::from(original))));
    }
    if message.extras.import_info.is_some() {
        text = format!(
            "This message was imported from another app. It may not be real.\n\n{text}"
        );
    }
    Some(text)
}
