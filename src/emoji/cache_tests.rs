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

/// An id the screen shows survives a merge that overflows the cap; the
/// oldest others go instead.
#[test]
fn visible_custom_emoji_survives_overflow() {
    use crate::state::{MessageReactionOptions, MessageReactionsState, ReactionChoice};
    let mut session = session();
    session.accept_custom_emoji_stickers(vec![resolved_emoji(Some(1), 1)]);
    session.stickers.message_reaction_options =
        MessageReactionsState::Ready(MessageReactionOptions {
            chat_id: crate::ids::ChatId(1),
            message_id: crate::ids::MessageId(1),
            top: vec![ReactionChoice::CustomEmoji(1)],
            recent: Vec::new(),
            popular: Vec::new(),
            allow_custom_emoji: true,
            premium_locked: Vec::new(),
        });
    let total = CUSTOM_EMOJI_CACHE_CAP as i64 + 5;
    session.accept_custom_emoji_stickers(
        (2..=total)
            .map(|id| resolved_emoji(Some(id), id as i32))
            .collect(),
    );
    let held = &session.stickers.emoji.custom_emoji_stickers;
    assert_eq!(held.len(), CUSTOM_EMOJI_CACHE_CAP);
    let ids = session.resolved_custom_emoji_ids();
    assert!(ids.contains(&1));
    assert!(!ids.contains(&2));
    assert!(ids.contains(&total));
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

/// What scanning the list for the first item with the id finds.
fn scanned(session: &Session, id: i64) -> Option<i32> {
    session
        .stickers
        .emoji
        .custom_emoji_stickers
        .iter()
        .find(|s| s.custom_emoji_id == Some(id))
        .map(|s| s.file_id.0)
}

fn looked_up(session: &Session, id: i64) -> Option<i32> {
    session
        .stickers
        .emoji
        .custom_emoji_stickers
        .lookup(id)
        .map(|s| s.file_id.0)
}

fn assert_lookup_matches_scan(session: &Session, ids: impl Iterator<Item = i64>) {
    for id in ids {
        assert_eq!(looked_up(session, id), scanned(session, id), "id {id}");
    }
}

#[test]
fn custom_emoji_lookup_matches_a_scan_after_inserts_and_replacements() {
    let mut session = session();
    assert_eq!(looked_up(&session, 1), None);
    session.accept_custom_emoji_stickers(vec![
        resolved_emoji(Some(1), 10),
        resolved_emoji(Some(2), 11),
        resolved_emoji(None, 12),
    ]);
    assert_eq!(looked_up(&session, 2), Some(11));
    assert_lookup_matches_scan(&session, 0..5);
    // A re-sent emoji replaces its entry and moves to the end.
    session.accept_custom_emoji_stickers(vec![
        resolved_emoji(Some(1), 20),
        resolved_emoji(Some(3), 13),
        resolved_emoji(Some(1), 21),
    ]);
    assert_eq!(looked_up(&session, 1), Some(21));
    assert_eq!(looked_up(&session, 3), Some(13));
    assert_lookup_matches_scan(&session, 0..5);
}

#[test]
fn custom_emoji_lookup_matches_a_scan_after_the_cache_is_trimmed() {
    let mut session = session();
    let total = CUSTOM_EMOJI_CACHE_CAP as i64 + 5;
    session.accept_custom_emoji_stickers(
        (2..=total)
            .map(|id| resolved_emoji(Some(id), id as i32))
            .collect(),
    );
    assert!(session.stickers.emoji.custom_emoji_stickers.len() <= CUSTOM_EMOJI_CACHE_CAP);
    // The oldest were dropped: the lookup no longer finds them either.
    assert_eq!(looked_up(&session, 2), None);
    assert_eq!(looked_up(&session, total), Some(total as i32));
    assert_lookup_matches_scan(&session, 0..=total + 1);
    // Adding more after the trim keeps the two in step.
    session.accept_custom_emoji_stickers(vec![resolved_emoji(Some(total + 1), 7)]);
    assert_eq!(looked_up(&session, total + 1), Some(7));
    assert_lookup_matches_scan(&session, 0..=total + 2);
}
