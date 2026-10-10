//! Screenshot demos that have no feature module of their own, grouped by
//! area. Each file registers its demos with `register_demos!`; a demo for a
//! feature with its own module belongs in that module instead (see
//! docs/decisions/codex-refactor-1.md).

mod bots_profile;
mod calls;
mod chat_list;
mod composer;
mod groups;
mod groups_admin;
mod media;
mod messages;
mod payments;
mod platform;
mod privacy_media;
mod security;
mod signin;
mod stories;

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

    /// Every kind `--screenshot-demo` accepted before demos registered
    /// themselves (docs/decisions/codex-refactor-1.md). New kinds need no
    /// entry here.
    const KINDS_BEFORE_REGISTRY: &str = include_str!("kinds_before_registry.txt");

    #[test]
    fn every_earlier_kind_is_still_registered() {
        let kinds: Vec<&str> = KINDS_BEFORE_REGISTRY.lines().collect();
        assert_eq!(kinds.len(), 280);
        for kind in kinds {
            assert_eq!(ScreenshotDemo::named(kind).kind(), kind);
        }
    }
}
