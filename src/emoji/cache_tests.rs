//! The resolved custom emoji cache: merging, bounding and lookups.
use super::{CUSTOM_EMOJI_CACHE_CAP, StickerItem};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::state::Session;
use std::sync::Arc;

fn session() -> Session {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    Session::new(AccountKey::primary(), sink)
}

fn resolved_emoji(custom_emoji_id: Option<i64>, file_id: i32) -> StickerItem {
    StickerItem {
        custom_emoji_id,
        id: i64::from(file_id),
        set_id: 2,
        emoji: "x".into(),
        width: 100,
        height: 100,
        format: crate::telegram::envelope::StickerFormat::Webp,
        file_id: crate::ids::FileId(file_id),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}

/// A re-sent custom emoji replaces its old entry; items without an id are
/// told apart by file.
#[test]
fn accepting_custom_emoji_stickers_replaces_by_id() {
    let mut session = session();
    session.accept_custom_emoji_stickers(vec![
        resolved_emoji(Some(1), 10),
        resolved_emoji(Some(2), 11),
        resolved_emoji(None, 12),
    ]);
    session.accept_custom_emoji_stickers(vec![
        resolved_emoji(Some(1), 20),
        resolved_emoji(None, 12),
        resolved_emoji(None, 13),
        resolved_emoji(Some(1), 21),
    ]);
    let held: Vec<(Option<i64>, i32)> = session
        .stickers
        .emoji
        .custom_emoji_stickers
        .iter()
        .map(|s| (s.custom_emoji_id, s.file_id.0))
        .collect();
    assert_eq!(
        held,
        vec![(Some(2), 11), (None, 12), (None, 13), (Some(1), 21)]
    );
    assert_eq!(
        session.resolved_custom_emoji_ids(),
        [1, 2].into_iter().collect()
    );
}

/// The cache keeps the newest entries and forgets the attempt mark of what
/// it drops, so a dropped id is requested again when it shows.
#[test]
fn custom_emoji_cache_is_bounded() {
    let mut session = session();
    let total = CUSTOM_EMOJI_CACHE_CAP as i64 + 5;
    session
        .stickers
        .emoji
        .status_resolution_attempted
        .extend(1..=total);
    let batch: Vec<StickerItem> = (1..=total)
        .map(|id| resolved_emoji(Some(id), id as i32))
        .collect();
    session.accept_custom_emoji_stickers(batch);
    let held = &session.stickers.emoji.custom_emoji_stickers;
    assert_eq!(held.len(), CUSTOM_EMOJI_CACHE_CAP);
    assert_eq!(held.first().and_then(|s| s.custom_emoji_id), Some(6));
    assert_eq!(held.last().and_then(|s| s.custom_emoji_id), Some(total));
    let attempted = &session.stickers.emoji.status_resolution_attempted;
    assert!(!attempted.contains(&1));
    assert!(attempted.contains(&total));
}

/// The ids still to resolve skip resolved and already attempted ones.
#[test]
fn unresolved_custom_emoji_ids_skip_resolved_and_attempted() {
    let mut session = session();
    session.accept_custom_emoji_stickers(vec![resolved_emoji(Some(2), 11)]);
    session.stickers.emoji.status_resolution_attempted.insert(3);
    assert_eq!(
        session.unresolved_custom_emoji_ids(vec![1, 2, 3, 4]),
        vec![1, 4]
    );
}
