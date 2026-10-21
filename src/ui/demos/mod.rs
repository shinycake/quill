//! Screenshot demos that have no feature module of their own, grouped by
//! area. Each file registers its demos with `register_demos!`; a demo for a
//! feature with its own module belongs in that module instead (see
//! docs/decisions/codex-refactor-1.md).
#![deny(clippy::indexing_slicing)]

mod bots_profile;
mod chat_list;
mod code_highlight;
mod common_groups;
mod composer;
mod derived_state;
mod group_call_bar;
mod groups_admin;
mod info_actions;
mod info_panels;
mod item_menus;
mod media;
mod menu_origin;
mod messages;
mod payments;
mod platform;
mod privacy_media;
mod security;
pub(crate) mod shared_media;
mod shared_media_search;
mod signin;
mod status_toasts;
mod stories;
mod topic_states;
pub(in crate::ui) mod video_fixtures;

use quill::composer::{AttachmentKind, ComposerAttachment};
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
        });
}

/// Composer attachments picked from the demo media folder, in order (a file
/// that is missing is skipped).
pub(in crate::ui) fn attachments(files: &[(&str, AttachmentKind)]) -> Vec<ComposerAttachment> {
    let mut pending = Vec::new();
    for (file, kind) in files {
        if let Some(att) =
            ComposerAttachment::pick(&super::demo::demo_media_allowlist().join(file), *kind)
        {
            ComposerAttachment::push_attachment(&mut pending, att);
        }
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::super::screenshot_demo::ScreenshotDemo;

    /// The kinds `--screenshot-demo` still accepts from before demos
    /// registered themselves (docs/decisions/codex-refactor-1.md): the 59 that
    /// scripts, tests or the README use. The other 233 were deleted
    /// (docs/decisions/codex-refactor-demo-prune.md). New kinds need no entry
    /// here.
    const KINDS_BEFORE_REGISTRY: &str = include_str!("kinds_before_registry.txt");

    #[test]
    fn every_earlier_kind_is_still_registered() {
        let kinds: Vec<&str> = KINDS_BEFORE_REGISTRY.lines().collect();
        assert_eq!(kinds.len(), 59);
        for kind in kinds {
            assert_eq!(ScreenshotDemo::named(kind).kind(), kind);
        }
    }
}
