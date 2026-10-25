//! The reaction options a demo message menu starts with.
use quill::ids::{ChatId, MessageId};
use quill::state::{MessageReactionOptions, MessageReactionsState, ReactionChoice, Session};

/// The reactions a demo message menu offers, as TDLib answers for a chat that
/// allows every emoji: seven quick ones in the strip, the rest behind the
/// chevron. Every message-menu demo seeds them through here.
pub(in crate::ui) fn seed_reaction_options(
    session: &mut Session,
    chat_id: ChatId,
    message_id: MessageId,
) {
    let emoji = |e: &&str| ReactionChoice::Emoji((*e).to_string());
    session.stickers.message_reaction_options =
        MessageReactionsState::Ready(MessageReactionOptions {
            chat_id,
            message_id,
            top: ["❤", "👍", "🔥", "😂", "😮", "😢", "🎉"]
                .iter()
                .map(emoji)
                .collect(),
            recent: vec![ReactionChoice::Emoji("👏".to_string())],
            popular: [
                "🤔", "🙏", "👌", "😍", "🤯", "😱", "🥰", "🤩", "💯", "⚡", "🏆", "🤝",
            ]
            .iter()
            .map(emoji)
            .collect(),
            allow_custom_emoji: false,
            premium_locked: Vec::new(),
        });
}
