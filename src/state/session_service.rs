//! Service-message wording backed by the session's caches (user and chat
//! names, loaded history for pinned excerpts and game titles).
use super::*;
use crate::service_text::{PinTarget, ServiceCtx, ServiceNames, ServiceText, pinned_media_phrase};

struct SessionNames<'a> {
    session: &'a Session,
    chat_id: i64,
}

impl ServiceNames for SessionNames<'_> {
    fn user_name(&self, user_id: i64) -> Option<String> {
        self.session
            .users
            .get(&user_id)
            .map(ParsedUser::display_name)
            .or_else(|| {
                // Private chat with the user: its title is their name.
                self.session
                    .chats
                    .get(&user_id)
                    .map(|chat| chat.title.clone())
            })
    }

    fn user_short_name(&self, user_id: i64) -> Option<String> {
        self.session.users.get(&user_id).map(|user| {
            if user.first_name.trim().is_empty() {
                user.display_name()
            } else {
                user.first_name.clone()
            }
        })
    }

    fn chat_name(&self, chat_id: i64) -> Option<String> {
        self.session
            .chats
            .get(&chat_id)
            .map(|chat| chat.title.clone())
    }

    fn pinned(&self, message_id: i64) -> PinTarget {
        // Loaded history first, else the copy `getRepliedMessage` fetched
        // for the pin row (a pin service message replies to the pinned one).
        let fetched =
            self.session
                .messages
                .reply_targets
                .values()
                .find_map(|target| match target {
                    ReplyTarget::Loaded(found)
                        if found.id.0 == message_id && found.chat_id.0 == self.chat_id =>
                    {
                        Some(found.as_ref())
                    }
                    _ => None,
                });
        let Some(message) = self
            .session
            .histories
            .get(&self.chat_id)
            .and_then(|history| history.messages.get(&message_id))
            .or(fetched)
        else {
            return PinTarget::Loading;
        };
        let content = effective_content(&message.content, message.ephemeral.as_ref());
        match (pinned_media_phrase(content), content) {
            (Some(phrase), _) => PinTarget::Media(phrase),
            (None, MessageContent::Text(text)) => PinTarget::Text(text.text.clone()),
            (None, other) => PinTarget::Text(other.preview()),
        }
    }

    fn game_title(&self, message_id: i64) -> Option<String> {
        match &self
            .session
            .histories
            .get(&self.chat_id)?
            .messages
            .get(&message_id)?
            .content
        {
            MessageContent::Game(game) => Some(game.title.clone()),
            _ => None,
        }
    }
}

impl Session {
    /// The wording of a service message (`None` for ordinary content),
    /// with names resolved from the caches.
    pub fn service_text_for(
        &self,
        chat_id: ChatId,
        content: &MessageContent,
        sender: Option<MessageSender>,
        is_outgoing: bool,
    ) -> Option<ServiceText> {
        let names = SessionNames {
            session: self,
            chat_id: chat_id.0,
        };
        let kind = self.chats.get(&chat_id.0).map(|chat| &chat.kind);
        let ctx = ServiceCtx {
            actor: sender,
            is_outgoing,
            my_id: self.my_user_id,
            is_channel: matches!(
                kind,
                Some(ChatKind::Supergroup {
                    is_channel: true,
                    ..
                })
            ),
            is_secret: matches!(kind, Some(ChatKind::Secret { .. })),
            chat_id: chat_id.0,
            now: crate::local_time::now_unix(),
            names: &names,
        };
        crate::service_text::render(content, &ctx)
    }

    /// Service wording of a history row.
    pub fn service_text(&self, message: &HistoryMessage) -> Option<ServiceText> {
        self.service_text_for(
            message.chat_id,
            &message.content,
            message.sender,
            message.is_outgoing,
        )
    }
}
