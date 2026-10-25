//! Reducer tests: the similar-channels strip under "You joined this channel".
use super::common::*;
use super::*;
use crate::telegram::envelope::ServiceAction;

const CHANNEL: &str = r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#;
const GROUP: &str = r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#;

fn join(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, chat_id: i64) {
    let extra = session.request(RequestPurpose::JoinChat, Some(ChatId(chat_id)));
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"chatJoinResultSuccess","@extra":"{}","chat_id":{chat_id}}}"#,
            extra.0
        ),
    );
}

fn similar_answer(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    chat_id: i64,
    ids: &[i64],
    total: i32,
) {
    let extra = session.request(
        RequestPurpose::GetProfileChats(ProfileChatsKind::SimilarChats),
        Some(ChatId(chat_id)),
    );
    let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":{total},"chat_ids":[{}]}}"#,
            extra.0,
            ids.join(",")
        ),
    );
}

#[test]
fn tiles_list_every_channel_when_nothing_is_beyond_the_list() {
    let tiles = similar_tiles(&[1, 2, 3], 3, false, 10);
    assert_eq!(
        tiles,
        vec![
            SimilarTile::Channel { chat_id: 1 },
            SimilarTile::Channel { chat_id: 2 },
            SimilarTile::Channel { chat_id: 3 },
        ]
    );
    // A server count that is stale (smaller than the list) adds nothing.
    assert_eq!(similar_tiles(&[1, 2, 3], 1, false, 10).len(), 3);
}

#[test]
fn the_last_tile_becomes_more_channels_locked_without_premium() {
    let list: Vec<i64> = (1..=10).collect();
    let tiles = similar_tiles(&list, 25, false, 10);
    assert_eq!(tiles.len(), 10);
    assert_eq!(tiles[8], SimilarTile::Channel { chat_id: 9 });
    assert_eq!(
        tiles[9],
        SimilarTile::More {
            count: 15,
            locked: true,
            stack: vec![10],
        }
    );
    // Premium: the same tile, unlocked.
    let premium = similar_tiles(&list, 25, true, 10);
    assert!(matches!(
        premium[9],
        SimilarTile::More { locked: false, .. }
    ));
}

#[test]
fn a_long_list_is_cut_at_the_limit_and_counts_the_rest() {
    // tdesktop: more than twice the limit cuts to the limit even when the
    // server counted nothing beyond the list; the stack shows the replaced
    // channel and the next two.
    let list: Vec<i64> = (1..=25).collect();
    let tiles = similar_tiles(&list, 25, true, 10);
    assert_eq!(tiles.len(), 10);
    assert_eq!(
        tiles[9],
        SimilarTile::More {
            count: 15,
            locked: false,
            stack: vec![10, 11, 12],
        }
    );
    // Up to twice the limit lists everything.
    let list: Vec<i64> = (1..=20).collect();
    assert_eq!(similar_tiles(&list, 20, true, 10).len(), 20);
}

#[test]
fn joining_a_channel_adds_the_notice_row_with_the_strip_expanded() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, CHANNEL);
    assert!(session.joined_notice_row(ChatId(13)).is_none());
    join(&mut session, &seq, &sink, 13);
    let strip = session.similar_strip(ChatId(13)).expect("strip");
    assert!(strip.expanded);
    assert!(strip.joined_at > 0);
    let row = session.joined_notice_row(ChatId(13)).expect("row");
    assert_eq!(row.id, MessageId(JOINED_NOTICE_ROW_ID));
    assert_eq!(row.date, strip.joined_at);
    assert!(matches!(
        &row.content,
        MessageContent::Action(action) if **action == ServiceAction::YouJoinedChannel
    ));
    assert_eq!(
        session.service_text(&row).map(|text| text.plain()),
        Some("You joined this channel".to_string())
    );
    // Not loaded yet: no tiles.
    assert_eq!(session.similar_strip_tiles(ChatId(13)), None);
}

#[test]
fn joining_a_group_shows_no_strip() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, GROUP);
    join(&mut session, &seq, &sink, 14);
    assert!(session.similar_strip(ChatId(14)).is_none());
    assert!(session.joined_notice_row(ChatId(14)).is_none());
}

#[test]
fn the_similar_chats_answer_fills_the_tiles_with_the_server_total() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, CHANNEL);
    join(&mut session, &seq, &sink, 13);
    similar_answer(&mut session, &seq, &sink, 13, &[101, 102, 103], 8);
    assert_eq!(session.users_state.similar_chat_totals.get(&13), Some(&8));
    let tiles = session.similar_strip_tiles(ChatId(13)).expect("tiles");
    assert_eq!(tiles.len(), 3);
    assert_eq!(
        tiles[2],
        SimilarTile::More {
            count: 5,
            locked: true,
            stack: vec![103],
        }
    );
    // An empty answer hides the strip (tdesktop `_empty`).
    let (mut session, sink) = super::common::session();
    apply_json(&mut session, &seq, &sink, CHANNEL);
    join(&mut session, &seq, &sink, 13);
    similar_answer(&mut session, &seq, &sink, 13, &[], 0);
    assert_eq!(session.similar_strip_tiles(ChatId(13)), Some(Vec::new()));
}

#[test]
fn clicking_the_notice_toggles_the_strip_and_rebuilds_the_rows() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, CHANNEL);
    join(&mut session, &seq, &sink, 13);
    similar_answer(&mut session, &seq, &sink, 13, &[101], 1);
    let revision = session.revision;
    assert_eq!(session.toggle_similar_strip(ChatId(13)), Some(false));
    assert_ne!(session.revision, revision, "rows rebuild on toggle");
    assert_eq!(session.similar_strip_tiles(ChatId(13)), None);
    // The notice itself stays while collapsed.
    assert!(session.joined_notice_row(ChatId(13)).is_some());
    assert_eq!(session.toggle_similar_strip(ChatId(13)), Some(true));
    assert_eq!(
        session.similar_strip_tiles(ChatId(13)).map(|t| t.len()),
        Some(1)
    );
    // Nothing to toggle in a chat that was not joined here.
    assert_eq!(session.toggle_similar_strip(ChatId(99)), None);
}

#[test]
fn the_strip_lists_as_many_channels_as_tdlib_allows_a_free_account() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, CHANNEL);
    join(&mut session, &seq, &sink, 13);
    similar_answer(&mut session, &seq, &sink, 13, &[101, 102, 103, 104], 9);
    session
        .payments
        .hub
        .limits
        .apply_premium_limit("premiumLimitTypeSimilarChatCount", 2, 100);
    let tiles = session.similar_strip_tiles(ChatId(13)).expect("tiles");
    assert_eq!(tiles.len(), 2);
    assert!(matches!(tiles[1], SimilarTile::More { count: 7, .. }));
}
