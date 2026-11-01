//! Screenshot demos that have no feature module of their own, grouped by
//! area. Each file registers its demos with `register_demos!`; a demo for a
//! feature with its own module belongs in that module instead (see
//! docs/decisions/codex-refactor-1.md).
#![deny(clippy::indexing_slicing)]

mod animated_emoji;
mod bots_profile;
mod chat_list;
mod code_highlight;
mod common_groups;
pub(in crate::ui) mod composer;
mod derived_state;
mod dice;
mod emoji_tap;
mod group_call_bar;
mod group_permissions;
pub(in crate::ui) mod group_photos;
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
mod profile_actions;
pub(crate) mod profile_cover;
pub(crate) mod reaction_fixtures;
mod security;
pub(crate) mod shared_media;
mod shared_media_search;
mod signin;
mod status_toasts;
mod stories;
mod topic_states;
pub(in crate::ui) mod video_avatar;
pub(in crate::ui) mod video_fixtures;
mod video_userpics;

use quill::composer::{AttachmentKind, ComposerAttachment};

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
